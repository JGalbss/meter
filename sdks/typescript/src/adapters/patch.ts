//! First-class auto-patch wrappers: monkey-patch a provider client so every call it makes is metered
//! automatically, with no change to existing call sites. Each wrapper returns an `Unpatch` that
//! restores the original method. Clients are duck-typed structurally, so the SDK never takes a hard
//! dependency on a provider's package.

import type { MeterClient } from "../client";
import type { Uuid } from "../types";
import { meterModelUsage, recordModelUsage } from "./instrument";
import {
  type AnthropicUsage,
  type OpenAiUsage,
  type TokenUsage,
  anthropicUsage,
  openaiUsage,
} from "./usage";

/** How a patched call reports usage: `"charge"` prices and debits credits; `"record"` only emits an event. */
export type PatchMode = "charge" | "record";

export interface PatchOptions {
  readonly orgId: Uuid;
  readonly account: Uuid;
  /** `"charge"` (default) prices + debits credits; `"record"` emits a usage event without charging. */
  readonly mode?: PatchMode;
  readonly runId?: Uuid;
  /** Force the model name; by default it is taken from the provider response, then the request. */
  readonly model?: string;
  /** Per-call idempotency key generator (default: a fresh UUID per call). */
  readonly idempotencyKey?: () => string;
  /** If set, a metering failure is routed here and the provider response is still returned (fail-open).
   *  If unset, a metering failure throws (matching `meteredCall`). */
  readonly onError?: (error: unknown) => void;
  /** Extra custom fields merged into the event properties (`"record"` mode only). */
  readonly extra?: Record<string, unknown>;
}

/** Restores the original, un-patched method. */
export type Unpatch = () => void;

function isRecordMode(mode: PatchMode | undefined): boolean {
  return mode === "record";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function asString(value: unknown): string | undefined {
  if (typeof value === "string") {
    return value;
  }
  return undefined;
}

/** The model named on the request body (`create({ model })`), used when the response omits it. */
function requestModel(args: readonly unknown[]): string | undefined {
  const [request] = args;
  if (!isRecord(request)) {
    return undefined;
  }
  return asString(request.model);
}

function newKey(options: PatchOptions): string {
  if (options.idempotencyKey !== undefined) {
    return options.idempotencyKey();
  }
  return crypto.randomUUID();
}

/** Spread-in the optional fields that are set, so we never pass explicit `undefined`
 *  (the SDK compiles under `exactOptionalPropertyTypes`). */
function optional(options: PatchOptions): { runId?: Uuid } {
  if (options.runId === undefined) {
    return {};
  }
  return { runId: options.runId };
}

function extraField(options: PatchOptions): { extra?: Record<string, unknown> } {
  if (options.extra === undefined) {
    return {};
  }
  return { extra: options.extra };
}

function emit(
  meter: MeterClient,
  options: PatchOptions,
  model: string,
  usage: TokenUsage,
): Promise<unknown> {
  const idempotencyKey = newKey(options);
  const base = { orgId: options.orgId, account: options.account, model, usage, idempotencyKey };
  if (isRecordMode(options.mode)) {
    return recordModelUsage(meter, { ...base, ...optional(options), ...extraField(options) });
  }
  return meterModelUsage(meter, { ...base, ...optional(options) });
}

/** A method holder whose `create` returns a response carrying usage and (optionally) a model name. */
interface Creator<R> {
  create: (...args: unknown[]) => Promise<R>;
}

/**
 * Wrap `holder.create` so each call meters its result, then returns the provider response unchanged.
 * `tokensOf` returns `undefined` when the response carries no usage (e.g. a streaming start), in which
 * case nothing is metered.
 */
function instrument<R>(
  holder: Creator<R>,
  meter: MeterClient,
  options: PatchOptions,
  tokensOf: (response: R) => TokenUsage | undefined,
  modelOf: (response: R) => string | undefined,
): Unpatch {
  const original = holder.create;
  holder.create = async (...args: unknown[]): Promise<R> => {
    const response = await original.apply(holder, args);
    await meterResponse(
      meter,
      options,
      response,
      tokensOf,
      modelOf(response) ?? requestModel(args),
    );
    return response;
  };
  return () => {
    holder.create = original;
  };
}

async function meterResponse<R>(
  meter: MeterClient,
  options: PatchOptions,
  response: R,
  tokensOf: (response: R) => TokenUsage | undefined,
  model: string | undefined,
): Promise<void> {
  try {
    const usage = tokensOf(response);
    if (usage === undefined || model === undefined) {
      return;
    }
    await emit(meter, options, options.model ?? model, usage);
  } catch (error) {
    if (options.onError === undefined) {
      throw error;
    }
    options.onError(error);
  }
}

/** A provider response that carries a usage block and (usually) the model that produced it. */
interface AnthropicResponse {
  readonly model?: string | null;
  readonly usage?: AnthropicUsage | null;
}

export interface AnthropicClientLike {
  readonly messages: Creator<AnthropicResponse>;
}

function anthropicTokens(response: AnthropicResponse): TokenUsage | undefined {
  if (response.usage === undefined || response.usage === null) {
    return undefined;
  }
  return anthropicUsage(response.usage);
}

/** Auto-meter every `messages.create` on an Anthropic / Claude client. Returns an `Unpatch`. */
export function patchAnthropic(
  meter: MeterClient,
  client: AnthropicClientLike,
  options: PatchOptions,
): Unpatch {
  return instrument(client.messages, meter, options, anthropicTokens, (r) => asString(r.model));
}

interface OpenAiResponse {
  readonly model?: string | null;
  readonly usage?: OpenAiUsage | null;
}

export interface OpenAiClientLike {
  readonly chat: { readonly completions: Creator<OpenAiResponse> };
}

function openaiTokens(response: OpenAiResponse): TokenUsage | undefined {
  if (response.usage === undefined || response.usage === null) {
    return undefined;
  }
  return openaiUsage(response.usage);
}

/** Auto-meter every `chat.completions.create` on an OpenAI client. Returns an `Unpatch`. */
export function patchOpenAI(
  meter: MeterClient,
  client: OpenAiClientLike,
  options: PatchOptions,
): Unpatch {
  return instrument(client.chat.completions, meter, options, openaiTokens, (r) =>
    asString(r.model),
  );
}
