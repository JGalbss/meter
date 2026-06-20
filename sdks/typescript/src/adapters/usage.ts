//! Provider-agnostic token-usage extraction.
//
// Each major AI client reports usage with a slightly different shape. These extractors normalize them
// to a single [`TokenUsage`]. They are structurally typed (no dependency on the provider SDKs), so
// they keep working across provider SDK versions.

/** Normalized token usage. Every count is non-negative; absent fields default to 0. */
export interface TokenUsage {
  readonly inputUncached: number;
  readonly cacheRead: number;
  readonly cacheWrite: number;
  readonly output: number;
  readonly reasoning: number;
}

/** A zero usage value. */
export const ZERO_USAGE: TokenUsage = {
  inputUncached: 0,
  cacheRead: 0,
  cacheWrite: 0,
  output: 0,
  reasoning: 0,
};

function count(value: unknown): number {
  if (typeof value === "number" && Number.isFinite(value) && value > 0) {
    return value;
  }
  return 0;
}

/** Anthropic / Claude (and the Claude Agent SDK) usage shape. */
export interface AnthropicUsage {
  readonly input_tokens?: number | null;
  readonly output_tokens?: number | null;
  readonly cache_creation_input_tokens?: number | null;
  readonly cache_read_input_tokens?: number | null;
}

/** Normalize Anthropic usage. `input_tokens` already excludes cached reads. */
export function anthropicUsage(usage: AnthropicUsage): TokenUsage {
  return {
    inputUncached: count(usage.input_tokens),
    cacheRead: count(usage.cache_read_input_tokens),
    cacheWrite: count(usage.cache_creation_input_tokens),
    output: count(usage.output_tokens),
    reasoning: 0,
  };
}

/** OpenAI usage shape (chat/responses APIs). */
export interface OpenAiUsage {
  readonly prompt_tokens?: number | null;
  readonly completion_tokens?: number | null;
  readonly prompt_tokens_details?: { readonly cached_tokens?: number | null } | null;
  readonly completion_tokens_details?: { readonly reasoning_tokens?: number | null } | null;
}

/** Normalize OpenAI usage. `prompt_tokens` includes cached tokens, so uncached = prompt − cached. */
export function openaiUsage(usage: OpenAiUsage): TokenUsage {
  const cached = count(usage.prompt_tokens_details?.cached_tokens);
  const prompt = count(usage.prompt_tokens);
  return {
    inputUncached: Math.max(0, prompt - cached),
    cacheRead: cached,
    cacheWrite: 0,
    output: count(usage.completion_tokens),
    reasoning: count(usage.completion_tokens_details?.reasoning_tokens),
  };
}

/** Vercel AI SDK usage shape (supports both the `promptTokens` and the `inputTokens` namings). */
export interface VercelAiUsage {
  readonly promptTokens?: number | null;
  readonly completionTokens?: number | null;
  readonly inputTokens?: number | null;
  readonly outputTokens?: number | null;
}

/** Normalize Vercel AI SDK usage. */
export function vercelAiUsage(usage: VercelAiUsage): TokenUsage {
  return {
    inputUncached: count(usage.inputTokens ?? usage.promptTokens),
    cacheRead: 0,
    cacheWrite: 0,
    output: count(usage.outputTokens ?? usage.completionTokens),
    reasoning: 0,
  };
}

/** Google Gemini / Vertex `usageMetadata` shape. */
export interface GeminiUsage {
  readonly promptTokenCount?: number | null;
  readonly candidatesTokenCount?: number | null;
  readonly cachedContentTokenCount?: number | null;
  readonly thoughtsTokenCount?: number | null;
}

/** Normalize Gemini/Vertex usage. `promptTokenCount` includes cached content. */
export function geminiUsage(usage: GeminiUsage): TokenUsage {
  const cached = count(usage.cachedContentTokenCount);
  const prompt = count(usage.promptTokenCount);
  return {
    inputUncached: Math.max(0, prompt - cached),
    cacheRead: cached,
    cacheWrite: 0,
    output: count(usage.candidatesTokenCount),
    reasoning: count(usage.thoughtsTokenCount),
  };
}

/** AWS Bedrock Converse API usage shape. */
export interface BedrockUsage {
  readonly inputTokens?: number | null;
  readonly outputTokens?: number | null;
  readonly cacheReadInputTokens?: number | null;
  readonly cacheWriteInputTokens?: number | null;
}

/** Normalize Bedrock usage. */
export function bedrockUsage(usage: BedrockUsage): TokenUsage {
  return {
    inputUncached: count(usage.inputTokens),
    cacheRead: count(usage.cacheReadInputTokens),
    cacheWrite: count(usage.cacheWriteInputTokens),
    output: count(usage.outputTokens),
    reasoning: 0,
  };
}
