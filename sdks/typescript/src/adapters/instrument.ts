//! Instrumentation: emit normalized model usage to meter, and wrap provider calls.

import type { MeterClient, RecordEventInput } from "../client";
import type { UsageEvent, Uuid } from "../types";
import type { TokenUsage } from "./usage";

export interface MeterModelUsageInput {
  readonly orgId: Uuid;
  readonly account: Uuid;
  readonly model: string;
  readonly usage: TokenUsage;
  readonly idempotencyKey: string;
  /** Meter name; defaults to `"tokens"`. */
  readonly meter?: string;
  readonly runId?: Uuid;
  /** Extra custom fields merged into the event properties. */
  readonly extra?: Record<string, unknown>;
}

/** Record normalized model token usage as a meter event (the OpenTelemetry-style emission). */
export function recordModelUsage(
  client: MeterClient,
  input: MeterModelUsageInput,
): Promise<UsageEvent> {
  const properties: Record<string, unknown> = {
    model: input.model,
    input_uncached: input.usage.inputUncached,
    cache_read: input.usage.cacheRead,
    cache_write: input.usage.cacheWrite,
    output: input.usage.output,
    reasoning: input.usage.reasoning,
    ...input.extra,
  };
  const base: RecordEventInput = {
    orgId: input.orgId,
    idempotencyKey: input.idempotencyKey,
    meter: input.meter ?? "tokens",
    account: input.account,
    properties,
  };
  if (input.runId !== undefined) {
    return client.recordEvent({ ...base, runId: input.runId });
  }
  return client.recordEvent(base);
}

/**
 * Wrap a provider call: run it, extract its usage with `extractUsage`, and record a meter event. The
 * provider's response is returned unchanged, so this drops into existing call sites.
 */
export async function meteredCall<R>(
  client: MeterClient,
  input: Omit<MeterModelUsageInput, "usage">,
  extractUsage: (response: R) => TokenUsage,
  call: () => Promise<R>,
): Promise<R> {
  const response = await call();
  await recordModelUsage(client, { ...input, usage: extractUsage(response) });
  return response;
}
