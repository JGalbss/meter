import { describe, expect, it, vi } from "vitest";

import { MeterClient, MeterError, withRun } from "../src/index";

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("MeterClient", () => {
  it("opens an account and maps camelCase input to the wire shape", async () => {
    const fetchMock = vi.fn(async (_url: string, _init?: RequestInit) =>
      jsonResponse(200, {
        id: "acc-1",
        org_id: "org-1",
        scope: "org",
        no_overdraft: true,
        parent_id: null,
      }),
    );
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const account = await client.openAccount({ orgId: "org-1", scope: "org", noOverdraft: true });

    expect(account.id).toBe("acc-1");
    const call = fetchMock.mock.calls[0];
    expect(call).toBeDefined();
    expect(call?.[0]).toBe("http://engine/v1/accounts");
    const init = (call?.[1] ?? {}) as RequestInit;
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toMatchObject({
      org_id: "org-1",
      scope: "org",
      no_overdraft: true,
    });
  });

  it("throws a MeterError carrying the engine code on a non-2xx response", async () => {
    const fetchMock = vi.fn(async () =>
      jsonResponse(404, { error: "not_found", message: "account not found" }),
    );
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const error = await client.balance("missing").catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(MeterError);
    expect(error).toMatchObject({ status: 404, code: "not_found" });
  });
});

describe("withRun", () => {
  it("reserves, runs work, and settles without voiding", async () => {
    const calls: string[] = [];
    const fetchMock = async (url: string, init?: RequestInit): Promise<Response> => {
      calls.push(`${init?.method ?? "GET"} ${url}`);
      if (url.endsWith("/reservations")) {
        return jsonResponse(200, { outcome: "allowed", reservation: "res-1" });
      }
      if (url.endsWith("/settle")) {
        return jsonResponse(200, { id: "entry-1" });
      }
      return jsonResponse(200, {});
    };
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const result = await withRun(
      client,
      { account: "acc-1", estimate: "40", reservationId: "res-1" },
      async (run) => {
        await run.settle("30");
        return "done";
      },
    );

    expect(result).toBe("done");
    expect(calls.some((entry) => entry.includes("/settle"))).toBe(true);
    expect(calls.some((entry) => entry.includes("/void"))).toBe(false);
  });

  it("voids the reservation when the work throws", async () => {
    const calls: string[] = [];
    const fetchMock = async (url: string, init?: RequestInit): Promise<Response> => {
      calls.push(`${init?.method ?? "GET"} ${url}`);
      if (url.endsWith("/reservations")) {
        return jsonResponse(200, { outcome: "allowed", reservation: "res-1" });
      }
      return new Response("", { status: 204 });
    };
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(
      withRun(client, { account: "acc-1", estimate: "40", reservationId: "res-1" }, async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");

    expect(calls.some((entry) => entry.includes("/void"))).toBe(true);
  });

  it("rejects with MeterError when the reservation is denied", async () => {
    const fetchMock = async (): Promise<Response> =>
      jsonResponse(200, { outcome: "denied", available: "5", requested: "40" });
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(
      withRun(
        client,
        { account: "acc-1", estimate: "40", reservationId: "res-1" },
        async () => "unused",
      ),
    ).rejects.toBeInstanceOf(MeterError);
  });
});
