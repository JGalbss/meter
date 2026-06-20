//! Read-only client for the Rust engine. The control plane never computes money — it asks the engine
//! to classify budget usage and reacts to the result. Boundary responses are validated with `Schema`.

import { Data, Effect, Schema } from "effect";

/** A failure talking to the engine. */
export class EngineError extends Data.TaggedError("EngineError")<{ readonly cause: unknown }> {}

const BudgetStatusSchema = Schema.Struct({
  used_credits: Schema.String,
  limit_credits: Schema.String,
  remaining_credits: Schema.String,
  ratio: Schema.String,
  status: Schema.Literal("ok", "warning", "exceeded"),
});

export type BudgetStatus = Schema.Schema.Type<typeof BudgetStatusSchema>;

// Building a codec compiles it; hoist it so each call reuses the same decoder.
const decodeBudgetStatus = Schema.decodeUnknown(BudgetStatusSchema);

export interface BudgetQuery {
  readonly accountId: string;
  readonly limit: string;
  readonly start: string;
  readonly end: string;
}

function engineUrl(): string {
  return process.env.METER_ENGINE_URL ?? "http://127.0.0.1:8080";
}

/** Classify an account's usage in a period against a credit limit (the engine owns this math). */
export function fetchBudgetStatus(query: BudgetQuery): Effect.Effect<BudgetStatus, EngineError> {
  return Effect.tryPromise({
    // Effect hands `tryPromise` an AbortSignal; forward it so fiber interruption cancels the fetch.
    try: async (signal) => {
      const params = new URLSearchParams({
        start: query.start,
        end: query.end,
        limit: query.limit,
      });
      const url = `${engineUrl()}/v1/accounts/${encodeURIComponent(query.accountId)}/budget?${params.toString()}`;
      const response = await fetch(url, { signal });
      if (!response.ok) {
        throw new Error(`engine responded ${response.status}`);
      }
      return await response.json();
    },
    catch: (cause) => new EngineError({ cause }),
  }).pipe(
    Effect.flatMap((body) =>
      decodeBudgetStatus(body).pipe(Effect.mapError((cause) => new EngineError({ cause }))),
    ),
  );
}
