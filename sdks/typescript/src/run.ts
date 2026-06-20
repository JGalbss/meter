//! Run governance: reserve before, settle after, void on failure.

import type { MeterClient } from "./client";
import { MeterError } from "./errors";
import { isDenied } from "./types";
import type { LimitClass, UsageTokens, Uuid } from "./types";

export interface RunHandle {
  readonly reservationId: Uuid;
  /** Settle the run with its actual credit usage. */
  settle(actual: string): Promise<void>;
}

export interface RunOptions {
  readonly account: Uuid;
  /** Worst-case credit estimate to reserve before the work runs. */
  readonly estimate: string;
  readonly reservationId?: Uuid;
  readonly limit?: LimitClass;
}

export interface UsageRunHandle {
  readonly reservationId: Uuid;
  /** Settle the run with its actual token usage (the engine reprices). */
  settle(actual: UsageTokens): Promise<void>;
}

export interface UsageRunOptions {
  readonly account: Uuid;
  readonly model: string;
  /** Worst-case token estimate to reserve before the work runs (priced by the engine). */
  readonly estimate: UsageTokens;
  readonly reservationId?: Uuid;
  readonly limit?: LimitClass;
}

/**
 * Run an agent operation under a credit reservation. The estimate is reserved up front; if the
 * reservation is denied the work never runs. `work` settles the actual usage via the handle; if it
 * throws (or never settles) the reservation is voided so a failed run leaves no lingering hold.
 */
export async function withRun<T>(
  client: MeterClient,
  options: RunOptions,
  work: (run: RunHandle) => Promise<T>,
): Promise<T> {
  const reservationId = options.reservationId ?? crypto.randomUUID();
  const outcome = await client.reserve({
    account: options.account,
    reservationId,
    amount: options.estimate,
    limit: options.limit ?? "hard",
  });
  if (isDenied(outcome)) {
    throw new MeterError(
      402,
      "reservation_denied",
      `reservation denied: ${outcome.available} available, ${outcome.requested} requested`,
    );
  }

  let settled = false;
  const handle: RunHandle = {
    reservationId,
    settle: async (actual: string): Promise<void> => {
      await client.settle(reservationId, actual);
      settled = true;
    },
  };

  try {
    const result = await work(handle);
    if (!settled) {
      await client.voidReservation(reservationId);
    }
    return result;
  } catch (error) {
    if (!settled) {
      await safeVoid(client, reservationId);
    }
    throw error;
  }
}

/**
 * Run an agent operation under a token-priced reservation. The token estimate is priced by the engine
 * and reserved up front; if denied the work never runs. `work` settles the actual token usage via the
 * handle; if it throws (or never settles) the reservation is voided so a failed run leaves no hold.
 */
export async function withRunUsage<T>(
  client: MeterClient,
  options: UsageRunOptions,
  work: (run: UsageRunHandle) => Promise<T>,
): Promise<T> {
  const reservationId = options.reservationId ?? crypto.randomUUID();
  const outcome = await client.reserveUsage({
    account: options.account,
    reservationId,
    model: options.model,
    estimate: options.estimate,
    limit: options.limit ?? "hard",
  });
  if (isDenied(outcome)) {
    throw new MeterError(
      402,
      "reservation_denied",
      `reservation denied: ${outcome.available} available, ${outcome.requested} requested`,
    );
  }

  let settled = false;
  const handle: UsageRunHandle = {
    reservationId,
    settle: async (actual: UsageTokens): Promise<void> => {
      await client.settleUsage(reservationId, { model: options.model, actual });
      settled = true;
    },
  };

  try {
    const result = await work(handle);
    if (!settled) {
      await client.voidReservation(reservationId);
    }
    return result;
  } catch (error) {
    if (!settled) {
      await safeVoid(client, reservationId);
    }
    throw error;
  }
}

async function safeVoid(client: MeterClient, reservationId: Uuid): Promise<void> {
  try {
    await client.voidReservation(reservationId);
  } catch {
    // Best-effort cleanup; the original error is surfaced by the caller.
  }
}
