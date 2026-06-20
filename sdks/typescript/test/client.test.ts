import { describe, expect, it, vi } from "vitest";

import { MeterClient, MeterError, withRun, withRunUsage } from "../src/index";

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

  it("opens and closes a lease", async () => {
    const calls: string[] = [];
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      calls.push(`${init?.method ?? "GET"} ${url}`);
      if (url.endsWith("/leases")) {
        return jsonResponse(200, {
          id: "lease-1",
          org_id: "org-1",
          scope: "session",
          no_overdraft: true,
          parent_id: "acc-1",
        });
      }
      return jsonResponse(200, { returned: "40" });
    });
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const lease = await client.openLease({ parent: "acc-1", amount: "60" });
    expect(lease.id).toBe("lease-1");
    expect(lease.scope).toBe("session");
    const openCall = fetchMock.mock.calls[0];
    expect(openCall?.[0]).toBe("http://engine/v1/leases");
    expect(JSON.parse((openCall?.[1] as RequestInit).body as string)).toEqual({
      parent: "acc-1",
      amount: "60",
    });

    const returned = await client.closeLease("lease-1");
    expect(returned).toBe("40");
    expect(calls).toContain("POST http://engine/v1/leases/lease-1/close");
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

  it("extends a reservation hold with the new expiry", async () => {
    const calls: Array<{ url: string; method: string; body: unknown }> = [];
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({
        url,
        method: init?.method ?? "GET",
        body: init?.body === undefined ? undefined : JSON.parse(init.body as string),
      });
      return new Response(null, { status: 204 });
    });
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await client.extendReservation("res-1", "2026-01-01T00:00:00Z");

    expect(calls).toHaveLength(1);
    expect(calls[0]?.method).toBe("POST");
    expect(calls[0]?.url).toBe("http://engine/v1/reservations/res-1/extend");
    expect(calls[0]?.body).toEqual({ expires_at: "2026-01-01T00:00:00Z" });
  });

  it("reserves and settles a usage-priced reservation", async () => {
    const calls: Array<{ url: string; body: unknown }> = [];
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({
        url,
        body: init?.body === undefined ? undefined : JSON.parse(init.body as string),
      });
      if (url.endsWith("/usage/reserve")) {
        return jsonResponse(200, {
          outcome: "allowed",
          reservation: "res-1",
          reserved_credits: "52500",
        });
      }
      return jsonResponse(200, { credits_charged: "50000", balance_after: "950000" });
    });
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const outcome = await client.reserveUsage({
      account: "acc-1",
      reservationId: "res-1",
      model: "claude-opus-4-8",
      estimate: { input_uncached: 1000, output: 500 },
      limit: "hard",
    });
    expect(outcome).toMatchObject({ outcome: "allowed", reserved_credits: "52500" });

    const settlement = await client.settleUsage("res-1", {
      model: "claude-opus-4-8",
      actual: { input_uncached: 900, output: 480 },
    });
    expect(settlement.credits_charged).toBe("50000");

    expect(calls[0]?.url).toBe("http://engine/v1/usage/reserve");
    expect(calls[0]?.body).toMatchObject({
      account: "acc-1",
      reservation_id: "res-1",
      model: "claude-opus-4-8",
      estimate: { input_uncached: 1000, output: 500 },
      limit: "hard",
    });
    expect(calls[1]?.url).toBe("http://engine/v1/usage/reservations/res-1/settle");
    expect(calls[1]?.body).toMatchObject({
      model: "claude-opus-4-8",
      actual: { input_uncached: 900, output: 480 },
    });
  });

  it("runs token-priced work under withRunUsage and settles without voiding", async () => {
    const calls: string[] = [];
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      calls.push(`${init?.method ?? "GET"} ${url}`);
      if (url.endsWith("/usage/reserve")) {
        return jsonResponse(200, {
          outcome: "allowed",
          reservation: "res-1",
          reserved_credits: "52500",
        });
      }
      return jsonResponse(200, { credits_charged: "50000", balance_after: "950000" });
    });
    const client = new MeterClient({
      baseUrl: "http://engine",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const result = await withRunUsage(
      client,
      {
        account: "acc-1",
        model: "claude-opus-4-8",
        estimate: { input_uncached: 1000, output: 500 },
        reservationId: "res-1",
      },
      async (run) => {
        await run.settle({ input_uncached: 900, output: 480 });
        return "done";
      },
    );

    expect(result).toBe("done");
    expect(calls.some((entry) => entry.includes("/usage/reserve"))).toBe(true);
    expect(calls.some((entry) => entry.includes("/usage/reservations/res-1/settle"))).toBe(true);
    expect(calls.some((entry) => entry.includes("/void"))).toBe(false);
  });
});
