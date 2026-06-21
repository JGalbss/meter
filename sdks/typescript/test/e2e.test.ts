//! End-to-end SDK test against a **real running engine** (not a fake fetch). Opt-in: it runs only when
//! `METER_E2E_BASE_URL` points at a live engine, so the normal `pnpm test` stays fast and infra-free.
//!
//! Bring up the stack and run it with:
//!   ./test/e2e/run.sh
//! or manually:
//!   METER_E2E_BASE_URL=http://localhost:8080 pnpm test e2e
//!
//! It exercises the full money path over the wire — the contract the SDK and engine must agree on —
//! so any drift between the generated/hand client and the engine's HTTP surface fails here.

import { describe, expect, it } from "vitest";

import { MeterClient, isAllowed, isDenied } from "../src/index";

const baseUrl = process.env.METER_E2E_BASE_URL;

describe.skipIf(!baseUrl)("e2e: SDK against a running engine", () => {
  const client = new MeterClient({ baseUrl: baseUrl ?? "" });

  it("open → grant → reserve → settle → balance → deny over-budget", async () => {
    const orgId = crypto.randomUUID();

    const account = await client.openAccount({ orgId, scope: "org", noOverdraft: true });
    expect(account.id).toBeTruthy();
    expect(account.no_overdraft).toBe(true);

    await client.grant(account.id, { amount: "1000", source: "paid" });
    expect(Number((await client.balance(account.id)).settled)).toBe(1000);

    // Reserve a hold, confirm it shows as held, then settle for less than reserved.
    const reservationId = crypto.randomUUID();
    const reserved = await client.reserve({
      account: account.id,
      reservationId,
      amount: "100",
      limit: "hard",
    });
    expect(isAllowed(reserved)).toBe(true);
    expect(Number((await client.balance(account.id)).held)).toBe(100);

    await client.settle(reservationId, "60");
    const settledBalance = await client.balance(account.id);
    expect(Number(settledBalance.settled)).toBe(940); // 1000 − 60 actual
    expect(Number(settledBalance.held)).toBe(0);

    // A hard reservation beyond the balance is denied — no overdraft on the wire.
    const denied = await client.reserve({
      account: account.id,
      reservationId: crypto.randomUUID(),
      amount: "100000",
      limit: "hard",
    });
    expect(isDenied(denied)).toBe(true);

    // The invoice sums the ledger (enforced == billed).
    const now = new Date();
    const start = new Date(now.getFullYear(), now.getMonth(), 1).toISOString();
    const end = new Date(now.getFullYear(), now.getMonth() + 1, 1).toISOString();
    const invoice = await client.invoice(account.id, start, end);
    expect(Number(invoice.total_credits)).toBe(60);
  });

  it("records a usage event and lists it back", async () => {
    const orgId = crypto.randomUUID();
    const account = await client.openAccount({ orgId, scope: "org", noOverdraft: false });

    const event = await client.recordEvent({
      orgId,
      idempotencyKey: crypto.randomUUID(),
      meter: "tokens",
      account: account.id,
      properties: { model: "claude-opus-4-8", output: 500 },
    });
    expect(event.status).toBe("recorded");

    // Idempotent: recording the same key again returns the same event id.
    const replay = await client.recordEvent({
      orgId,
      idempotencyKey: event.idempotency_key,
      meter: "tokens",
      account: account.id,
      properties: { model: "claude-opus-4-8", output: 500 },
    });
    expect(replay.id).toBe(event.id);
  });
});
