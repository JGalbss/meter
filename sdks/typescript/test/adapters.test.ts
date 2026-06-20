import { describe, expect, it } from "vitest";

import {
  MeterClient,
  anthropicUsage,
  bedrockUsage,
  geminiUsage,
  meteredCall,
  openaiUsage,
  recordModelUsage,
  vercelAiUsage,
} from "../src/index";

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("usage extractors", () => {
  it("normalizes Anthropic usage (input excludes cache reads)", () => {
    expect(
      anthropicUsage({
        input_tokens: 1000,
        output_tokens: 500,
        cache_read_input_tokens: 200,
        cache_creation_input_tokens: 50,
      }),
    ).toEqual({ inputUncached: 1000, cacheRead: 200, cacheWrite: 50, output: 500, reasoning: 0 });
  });

  it("normalizes OpenAI usage (uncached = prompt − cached; reasoning surfaced)", () => {
    expect(
      openaiUsage({
        prompt_tokens: 1000,
        completion_tokens: 500,
        prompt_tokens_details: { cached_tokens: 200 },
        completion_tokens_details: { reasoning_tokens: 120 },
      }),
    ).toEqual({ inputUncached: 800, cacheRead: 200, cacheWrite: 0, output: 500, reasoning: 120 });
  });

  it("normalizes Vercel AI SDK usage in both namings", () => {
    expect(vercelAiUsage({ promptTokens: 10, completionTokens: 7 })).toMatchObject({
      inputUncached: 10,
      output: 7,
    });
    expect(vercelAiUsage({ inputTokens: 11, outputTokens: 8 })).toMatchObject({
      inputUncached: 11,
      output: 8,
    });
  });

  it("normalizes Gemini/Vertex usage (prompt includes cached content)", () => {
    expect(
      geminiUsage({
        promptTokenCount: 1000,
        candidatesTokenCount: 400,
        cachedContentTokenCount: 250,
        thoughtsTokenCount: 60,
      }),
    ).toEqual({ inputUncached: 750, cacheRead: 250, cacheWrite: 0, output: 400, reasoning: 60 });
  });

  it("normalizes Bedrock Converse usage", () => {
    expect(
      bedrockUsage({
        inputTokens: 900,
        outputTokens: 300,
        cacheReadInputTokens: 100,
        cacheWriteInputTokens: 20,
      }),
    ).toEqual({ inputUncached: 900, cacheRead: 100, cacheWrite: 20, output: 300, reasoning: 0 });
  });
});

describe("recordModelUsage", () => {
  it("emits a usage event with normalized properties", async () => {
    let body: Record<string, unknown> | undefined;
    const fetchMock = async (_url: string, init?: RequestInit): Promise<Response> => {
      body = JSON.parse((init?.body as string) ?? "{}") as Record<string, unknown>;
      return jsonResponse(200, { id: "evt-1", status: "recorded" });
    };
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await recordModelUsage(client, {
      orgId: "org-1",
      account: "acc-1",
      model: "claude-opus-4-8",
      idempotencyKey: "u1",
      runId: "run-1",
      usage: anthropicUsage({ input_tokens: 1000, output_tokens: 500 }),
    });

    expect(body).toMatchObject({
      meter: "tokens",
      account: "acc-1",
      run_id: "run-1",
      properties: { model: "claude-opus-4-8", input_uncached: 1000, output: 500 },
    });
  });
});

describe("meteredCall", () => {
  it("runs the provider call and records its usage", async () => {
    const recorded: string[] = [];
    const fetchMock = async (url: string): Promise<Response> => {
      recorded.push(url);
      return jsonResponse(200, { id: "evt-1" });
    };
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const providerResponse = { usage: { prompt_tokens: 100, completion_tokens: 50 } };
    const result = await meteredCall(
      client,
      { orgId: "org-1", account: "acc-1", model: "gpt-x", idempotencyKey: "u2" },
      (response: typeof providerResponse) => openaiUsage(response.usage),
      async () => providerResponse,
    );

    expect(result).toBe(providerResponse);
    expect(recorded.some((url) => url.endsWith("/v1/events"))).toBe(true);
  });
});
