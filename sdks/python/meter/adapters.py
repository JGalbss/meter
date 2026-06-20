"""Adapters that auto-instrument the major AI clients to emit usage to meter.

Provider-agnostic and structurally typed (read the usage dict), so they keep working across provider
SDK versions.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, TypeVar

from .client import MeterClient

R = TypeVar("R")


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
    """Normalize Anthropic / Claude usage. `input_tokens` already excludes cache reads."""
    return TokenUsage(
        input_uncached=_count(usage.get("input_tokens")),
        cache_read=_count(usage.get("cache_read_input_tokens")),
        cache_write=_count(usage.get("cache_creation_input_tokens")),
        output=_count(usage.get("output_tokens")),
    )


def openai_usage(usage: dict[str, Any]) -> TokenUsage:
    """Normalize OpenAI usage. `prompt_tokens` includes cached, so uncached = prompt - cached."""
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


def gemini_usage(usage: dict[str, Any]) -> TokenUsage:
    """Normalize Google Gemini / Vertex usageMetadata. promptTokenCount includes cached content."""
    cached = _count(usage.get("cachedContentTokenCount"))
    prompt = _count(usage.get("promptTokenCount"))
    return TokenUsage(
        input_uncached=max(0, prompt - cached),
        cache_read=cached,
        output=_count(usage.get("candidatesTokenCount")),
        reasoning=_count(usage.get("thoughtsTokenCount")),
    )


def langchain_usage(usage: dict[str, Any]) -> TokenUsage:
    """Normalize LangChain / LangGraph usage_metadata. input_tokens includes cached reads/writes."""
    input_details = usage.get("input_token_details") or {}
    output_details = usage.get("output_token_details") or {}
    cache_read = _count(input_details.get("cache_read"))
    cache_write = _count(input_details.get("cache_creation"))
    total_input = _count(usage.get("input_tokens"))
    return TokenUsage(
        input_uncached=max(0, total_input - cache_read - cache_write),
        cache_read=cache_read,
        cache_write=cache_write,
        output=_count(usage.get("output_tokens")),
        reasoning=_count(output_details.get("reasoning")),
    )


def bedrock_usage(usage: dict[str, Any]) -> TokenUsage:
    """Normalize AWS Bedrock Converse usage."""
    return TokenUsage(
        input_uncached=_count(usage.get("inputTokens")),
        cache_read=_count(usage.get("cacheReadInputTokens")),
        cache_write=_count(usage.get("cacheWriteInputTokens")),
        output=_count(usage.get("outputTokens")),
    )


def meter_model_usage(
    client: MeterClient,
    *,
    org_id: str,
    account: str,
    model: str,
    usage: TokenUsage,
    idempotency_key: str,
    run_id: str | None = None,
) -> dict[str, Any]:
    """Price + charge normalized model usage in one idempotent call (record + debit)."""
    return client.meter_usage(
        org_id=org_id,
        account=account,
        model=model,
        idempotency_key=idempotency_key,
        run_id=run_id,
        usage={
            "input_uncached": usage.input_uncached,
            "cache_read": usage.cache_read,
            "cache_write": usage.cache_write,
            "output": usage.output,
            "reasoning": usage.reasoning,
        },
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
    run_id: str | None = None,
    extra: dict[str, Any] | None = None,
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


def metered_call(
    client: MeterClient,
    *,
    org_id: str,
    account: str,
    model: str,
    idempotency_key: str,
    extract_usage: Callable[[R], TokenUsage],
    call: Callable[[], R],
    meter: str = "tokens",
    run_id: str | None = None,
    extra: dict[str, Any] | None = None,
) -> R:
    """Wrap a provider call: run it, extract its usage, and record a meter event.

    The provider's response is returned unchanged, so this drops into existing call sites.
    """
    response = call()
    record_model_usage(
        client,
        org_id=org_id,
        account=account,
        model=model,
        usage=extract_usage(response),
        idempotency_key=idempotency_key,
        meter=meter,
        run_id=run_id,
        extra=extra,
    )
    return response
