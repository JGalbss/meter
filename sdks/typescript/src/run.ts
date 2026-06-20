//! Run governance: reserve before, settle after, void on failure.

import type { MeterClient } from "./client";
import { MeterError } from "./errors";
import { isDenied } from "./types";
import type { LimitClass, Uuid } from "./types";

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

async function safeVoid(client: MeterClient, reservationId: Uuid): Promise<void> {
  try {
    await client.voidReservation(reservationId);
  } catch {
    // Best-effort cleanup; the original error is surfaced by the caller.
  }
}
