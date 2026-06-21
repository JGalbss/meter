"""First-class auto-patch wrappers: monkey-patch a provider client so every call it makes is metered
automatically, with no change to existing call sites.

Each wrapper returns an ``Unpatch`` callable that restores the original method. Provider clients are
duck-typed structurally (usage read from a dict or an object), so the SDK never imports a provider
package and keeps working across provider SDK versions.
"""

from __future__ import annotations

import uuid
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from .adapters import (
    TokenUsage,
    anthropic_usage,
    meter_model_usage,
    openai_usage,
    record_model_usage,
)
from .client import MeterClient

Unpatch = Callable[[], None]
Normalize = Callable[[dict[str, Any]], TokenUsage]


@dataclass(frozen=True)
class _Config:
    org_id: str
    account: str
    mode: str
    run_id: str | None
    model: str | None
    idempotency_key: Callable[[], str] | None
    on_error: Callable[[BaseException], None] | None
    extra: dict[str, Any] | None


def _is_record(mode: str) -> bool:
    return mode == "record"


def _field(obj: Any, name: str) -> Any:
    if isinstance(obj, dict):
        return obj.get(name)
    return getattr(obj, name, None)


def _as_usage_dict(usage: Any) -> dict[str, Any] | None:
    """Coerce a usage value (dict, pydantic model, or plain object) to a plain dict."""
    if usage is None:
        return None
    if isinstance(usage, dict):
        return usage
    dump = getattr(usage, "model_dump", None)
    if callable(dump):
        return dump()
    return vars(usage)


def _request_model(args: tuple[Any, ...], kwargs: dict[str, Any]) -> str | None:
    """The model named on the request (`create(model=...)`), used when the response omits it."""
    if "model" in kwargs:
        return kwargs["model"]
    if args and isinstance(args[0], dict):
        return args[0].get("model")
    return None


def _next_key(config: _Config) -> str:
    if config.idempotency_key is not None:
        return config.idempotency_key()
    return str(uuid.uuid4())


def _emit(meter: MeterClient, config: _Config, model: str, usage: TokenUsage) -> None:
    key = _next_key(config)
    if _is_record(config.mode):
        record_model_usage(
            meter,
            org_id=config.org_id,
            account=config.account,
            model=model,
            usage=usage,
            idempotency_key=key,
            run_id=config.run_id,
            extra=config.extra,
        )
        return
    meter_model_usage(
        meter,
        org_id=config.org_id,
        account=config.account,
        model=model,
        usage=usage,
        idempotency_key=key,
        run_id=config.run_id,
    )


def _instrument(
    holder: Any,
    method: str,
    normalize: Normalize,
    meter: MeterClient,
    config: _Config,
) -> Unpatch:
    original = getattr(holder, method)

    def wrapped(*args: Any, **kwargs: Any) -> Any:
        response = original(*args, **kwargs)
        _meter_response(response, args, kwargs, normalize, meter, config)
        return response

    setattr(holder, method, wrapped)

    def unpatch() -> None:
        setattr(holder, method, original)

    return unpatch


def _meter_response(
    response: Any,
    args: tuple[Any, ...],
    kwargs: dict[str, Any],
    normalize: Normalize,
    meter: MeterClient,
    config: _Config,
) -> None:
    try:
        usage_raw = _as_usage_dict(_field(response, "usage"))
        model = config.model or _field(response, "model") or _request_model(args, kwargs)
        if usage_raw is None or model is None:
            return
        _emit(meter, config, model, normalize(usage_raw))
    except Exception as error:  # noqa: BLE001 — metering must not crash the wrapped call
        if config.on_error is None:
            raise
        config.on_error(error)


def _config(
    *,
    org_id: str,
    account: str,
    mode: str,
    run_id: str | None,
    model: str | None,
    idempotency_key: Callable[[], str] | None,
    on_error: Callable[[BaseException], None] | None,
    extra: dict[str, Any] | None,
) -> _Config:
    return _Config(
        org_id=org_id,
        account=account,
        mode=mode,
        run_id=run_id,
        model=model,
        idempotency_key=idempotency_key,
        on_error=on_error,
        extra=extra,
    )


def patch_anthropic(
    meter: MeterClient,
    provider: Any,
    *,
    org_id: str,
    account: str,
    mode: str = "charge",
    run_id: str | None = None,
    model: str | None = None,
    idempotency_key: Callable[[], str] | None = None,
    on_error: Callable[[BaseException], None] | None = None,
    extra: dict[str, Any] | None = None,
) -> Unpatch:
    """Auto-meter every ``messages.create`` on an Anthropic / Claude client. Returns an ``Unpatch``.

    ``mode="charge"`` (default) prices and debits credits; ``mode="record"`` emits a usage event
    without charging. A metering failure raises unless ``on_error`` is given (then it is fail-open
    and the provider response is still returned).
    """
    config = _config(
        org_id=org_id,
        account=account,
        mode=mode,
        run_id=run_id,
        model=model,
        idempotency_key=idempotency_key,
        on_error=on_error,
        extra=extra,
    )
    return _instrument(provider.messages, "create", anthropic_usage, meter, config)


def patch_openai(
    meter: MeterClient,
    provider: Any,
    *,
    org_id: str,
    account: str,
    mode: str = "charge",
    run_id: str | None = None,
    model: str | None = None,
    idempotency_key: Callable[[], str] | None = None,
    on_error: Callable[[BaseException], None] | None = None,
    extra: dict[str, Any] | None = None,
) -> Unpatch:
    """Auto-meter every ``chat.completions.create`` on an OpenAI client. Returns an ``Unpatch``.

    See :func:`patch_anthropic` for the shared options.
    """
    config = _config(
        org_id=org_id,
        account=account,
        mode=mode,
        run_id=run_id,
        model=model,
        idempotency_key=idempotency_key,
        on_error=on_error,
        extra=extra,
    )
    return _instrument(provider.chat.completions, "create", openai_usage, meter, config)
