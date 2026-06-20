import { describe, expect, it } from "vitest";

import {
  MeterClient,
  anthropicUsage,
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
