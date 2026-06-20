"""Adapters that auto-instrument the major AI clients to emit usage to meter.

Provider-agnostic and structurally typed (read the usage dict), so they keep working across provider
SDK versions.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional

from .client import MeterClient


@dataclass(frozen=True)
class TokenUsage:
    """Normalized token usage. Every count is non-negative; absent fields default to 0."""

    input_uncached: int = 0
    cache_read: int = 0
    cache_write: int = 0
    output: int = 0
    reasoning: int = 0


def _count(value: Any) -> int:
    if isinstance(value, bool):
        return 0
    if isinstance(value, (int, float)) and value > 0:
        return int(value)
    return 0


def anthropic_usage(usage: dict[str, Any]) -> TokenUsage:
    """Normalize Anthropic / Claude (and Claude Agent SDK) usage. `input_tokens` excludes cache reads."""
    return TokenUsage(
        input_uncached=_count(usage.get("input_tokens")),
        cache_read=_count(usage.get("cache_read_input_tokens")),
        cache_write=_count(usage.get("cache_creation_input_tokens")),
        output=_count(usage.get("output_tokens")),
    )


def openai_usage(usage: dict[str, Any]) -> TokenUsage:
    """Normalize OpenAI usage. `prompt_tokens` includes cached tokens, so uncached = prompt - cached."""
    prompt_details = usage.get("prompt_tokens_details") or {}
    completion_details = usage.get("completion_tokens_details") or {}
    cached = _count(prompt_details.get("cached_tokens"))
    prompt = _count(usage.get("prompt_tokens"))
    return TokenUsage(
        input_uncached=max(0, prompt - cached),
        cache_read=cached,
        output=_count(usage.get("completion_tokens")),
        reasoning=_count(completion_details.get("reasoning_tokens")),
    )


def record_model_usage(
    client: MeterClient,
    *,
    org_id: str,
    account: str,
    model: str,
    usage: TokenUsage,
    idempotency_key: str,
    meter: str = "tokens",
    run_id: Optional[str] = None,
    extra: Optional[dict[str, Any]] = None,
) -> dict[str, Any]:
    """Record normalized model token usage as a meter event (the OpenTelemetry-style emission)."""
    properties: dict[str, Any] = {
        "model": model,
        "input_uncached": usage.input_uncached,
        "cache_read": usage.cache_read,
        "cache_write": usage.cache_write,
        "output": usage.output,
        "reasoning": usage.reasoning,
    }
    if extra:
        properties.update(extra)
    return client.record_event(
        org_id=org_id,
        idempotency_key=idempotency_key,
        meter=meter,
        account=account,
        run_id=run_id,
        properties=properties,
    )
