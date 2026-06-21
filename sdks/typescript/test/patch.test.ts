import { describe, expect, it } from "vitest";

import { MeterClient, patchAnthropic, patchOpenAI } from "../src/index";

interface Captured {
  readonly url: string;
  readonly body: Record<string, unknown>;
}

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/** A client whose fetch records every request and answers usage/event calls with canned bodies. */
function recordingClient(): { client: MeterClient; calls: Captured[] } {
  const calls: Captured[] = [];
  const fetchMock = async (url: string, init?: RequestInit): Promise<Response> => {
    calls.push({
      url,
      body: JSON.parse((init?.body as string) ?? "{}") as Record<string, unknown>,
    });
    return jsonResponse(200, {
      credits: "52500",
      charged: true,
      settled: "947500",
      available: "947500",
      event_id: "evt-1",
      id: "evt-1",
      status: "recorded",
    });
  };
  const client = new MeterClient({
    baseUrl: "http://engine",
    fetch: fetchMock as unknown as typeof fetch,
  });
  return { client, calls };
}

describe("patchAnthropic", () => {
  it("meters every messages.create and returns the response unchanged", async () => {
    const { client, calls } = recordingClient();
    const anthropic = {
      messages: {
        create: async (_request: unknown) => ({
          model: "claude-opus-4-8",
          usage: { input_tokens: 1000, output_tokens: 500, cache_read_input_tokens: 200 },
          content: "hi",
        }),
      },
    };

    const unpatch = patchAnthropic(client, anthropic, { orgId: "org-1", account: "acc-1" });
    const response = await anthropic.messages.create({ model: "claude-opus-4-8" });

    // Provider response is untouched.
    expect(response.content).toBe("hi");
    // One charge call to /v1/usage with normalized tokens and the response's model.
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe("http://engine/v1/usage");
    expect(calls[0]?.body).toMatchObject({
      account: "acc-1",
      model: "claude-opus-4-8",
      usage: { input_uncached: 1000, cache_read: 200, output: 500 },
    });
    expect(typeof calls[0]?.body.idempotency_key).toBe("string");

    unpatch();
    await anthropic.messages.create({ model: "claude-opus-4-8" });
    // After unpatch, no further metering.
    expect(calls).toHaveLength(1);
  });

  it("generates a fresh idempotency key per call", async () => {
    const { client, calls } = recordingClient();
    const anthropic = {
      messages: {
        create: async () => ({
          model: "claude-opus-4-8",
          usage: { input_tokens: 1, output_tokens: 1 },
        }),
      },
    };
    patchAnthropic(client, anthropic, { orgId: "org-1", account: "acc-1" });
    await anthropic.messages.create();
    await anthropic.messages.create();
    expect(calls).toHaveLength(2);
    expect(calls[0]?.body.idempotency_key).not.toBe(calls[1]?.body.idempotency_key);
  });

  it("falls back to the request model when the response omits it", async () => {
    const { client, calls } = recordingClient();
    const anthropic = {
      messages: {
        create: async (_r: unknown) => ({ usage: { input_tokens: 1, output_tokens: 1 } }),
      },
    };
    patchAnthropic(client, anthropic, { orgId: "org-1", account: "acc-1" });
    await anthropic.messages.create({ model: "claude-haiku-4-5" });
    expect(calls[0]?.body).toMatchObject({ model: "claude-haiku-4-5" });
  });

  it("does not meter a response that carries no usage", async () => {
    const { client, calls } = recordingClient();
    const anthropic = { messages: { create: async () => ({ model: "claude-opus-4-8" }) } };
    patchAnthropic(client, anthropic, { orgId: "org-1", account: "acc-1" });
    await anthropic.messages.create();
    expect(calls).toHaveLength(0);
  });
});

describe("patchOpenAI", () => {
  it("records (without charging) in record mode and emits an event", async () => {
    const { client, calls } = recordingClient();
    const openai = {
      chat: {
        completions: {
          create: async () => ({
            model: "gpt-4o",
            usage: {
              prompt_tokens: 1000,
              completion_tokens: 500,
              prompt_tokens_details: { cached_tokens: 200 },
            },
          }),
        },
      },
    };

    patchOpenAI(client, openai, {
      orgId: "org-1",
      account: "acc-1",
      mode: "record",
      extra: { team: "research" },
    });
    await openai.chat.completions.create();

    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe("http://engine/v1/events");
    expect(calls[0]?.body).toMatchObject({
      org_id: "org-1",
      account: "acc-1",
      meter: "tokens",
      properties: {
        model: "gpt-4o",
        input_uncached: 800,
        cache_read: 200,
        output: 500,
        team: "research",
      },
    });
  });
});

describe("patch error handling", () => {
  it("rethrows a metering failure when no onError is given", async () => {
    const failing = new MeterClient({
      baseUrl: "http://engine",
      fetch: (async () => jsonResponse(500, { error: "boom" })) as unknown as typeof fetch,
    });
    const anthropic = {
      messages: {
        create: async () => ({ model: "m", usage: { input_tokens: 1, output_tokens: 1 } }),
      },
    };
    patchAnthropic(failing, anthropic, { orgId: "org-1", account: "acc-1" });
    await expect(anthropic.messages.create()).rejects.toThrow();
  });

  it("is fail-open when onError is given: the provider response still returns", async () => {
    let captured: unknown;
    const failing = new MeterClient({
      baseUrl: "http://engine",
      fetch: (async () => jsonResponse(500, { error: "boom" })) as unknown as typeof fetch,
    });
    const anthropic = {
      messages: {
        create: async () => ({
          model: "m",
          usage: { input_tokens: 1, output_tokens: 1 },
          ok: true,
        }),
      },
    };
    patchAnthropic(failing, anthropic, {
      orgId: "org-1",
      account: "acc-1",
      onError: (error) => {
        captured = error;
      },
    });
    const response = await anthropic.messages.create();
    expect(response.ok).toBe(true);
    expect(captured).toBeDefined();
  });
});
