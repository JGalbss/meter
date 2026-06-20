//! Wire types for the meter engine API.
//
// Hand-written for now; these will be generated from the engine's OpenAPI once it is emitted.
// Credit/money amounts are exact decimal strings (never JS numbers) to avoid float drift.

/** A UUID string. */
export type Uuid = string;

/** Provenance of granted credits. */
export type CreditSource = "paid" | "promo" | "grant";

/** Whether a limit blocks hard (never overdraft) or soft (best-effort). */
export type LimitClass = "hard" | "soft";

export interface Account {
  readonly id: Uuid;
  readonly org_id: Uuid;
  readonly scope: string;
  readonly no_overdraft: boolean;
  readonly parent_id: Uuid | null;
}

export interface Balance {
  /** Settled credits (decimal string). */
  readonly settled: string;
  /** Credits held by open reservations (decimal string). */
  readonly held: string;
}

export interface LedgerEntry {
  readonly id: Uuid;
  readonly account_id: Uuid;
  readonly paired_account_id: Uuid;
  readonly entry_type: string;
  readonly delta_credits: string;
  readonly balance_after: string;
  readonly created_at: string;
}

export type ReserveOutcome =
  | { readonly outcome: "allowed"; readonly reservation: Uuid }
  | { readonly outcome: "denied"; readonly available: string; readonly requested: string };

/** Whether a reservation was allowed (type guard, avoids bare `===` at call sites). */
export function isAllowed(
  outcome: ReserveOutcome,
): outcome is Extract<ReserveOutcome, { outcome: "allowed" }> {
  return outcome.outcome === "allowed";
}

/** Whether a reservation was denied. */
export function isDenied(
  outcome: ReserveOutcome,
): outcome is Extract<ReserveOutcome, { outcome: "denied" }> {
  return outcome.outcome === "denied";
}

export interface UsageEvent {
  readonly id: Uuid;
  readonly org_id: Uuid;
  readonly idempotency_key: string;
  readonly event_time: string;
  readonly meter: string;
  readonly account_id: Uuid;
  readonly run_id: Uuid | null;
  readonly properties: Record<string, unknown>;
  readonly status: string;
  readonly supersedes: Uuid | null;
  readonly created_at: string;
}

export interface Invoice {
  readonly account_id: Uuid;
  readonly total_credits: string;
  readonly settle_count: number;
}

/** Token counts sent to the usage-metering endpoint (the wire shape). */
export interface UsageTokens {
  readonly input_uncached?: number;
  readonly cache_read?: number;
  readonly cache_write?: number;
  readonly output?: number;
  readonly reasoning?: number;
}

/** The result of metering usage: the priced credits, whether it charged, and the new balance. */
export interface UsageResult {
  readonly credits: string;
  readonly cogs_usd: string;
  readonly customer_price_usd: string;
  readonly event_id: Uuid;
  readonly charged: boolean;
  readonly settled: string;
  readonly available: string;
}

/** A usage-priced reservation outcome: the reserve outcome plus the credits the estimate priced to. */
export type UsageReserveOutcome = ReserveOutcome & { readonly reserved_credits: string };

/** The result of settling a usage-priced reservation against actual usage. */
export interface UsageSettlement {
  readonly credits_charged: string;
  readonly balance_after: string;
}

/** One catalogued model's provider-cost prices (exact decimal strings, per token). */
export interface CatalogModel {
  readonly provider: string;
  readonly model_id: string;
  readonly input_per_token: string;
  readonly cache_read_per_token: string;
  readonly cache_write_per_token: string;
  readonly output_per_token: string;
}

/** The hosted rate-card catalog: a dated snapshot of model prices. */
export interface Catalog {
  readonly as_of: string;
  readonly models: readonly CatalogModel[];
}

/** The result of re-rating a usage stream from one model onto another. */
export interface SimulateResult {
  readonly current_model: string;
  readonly proposed_model: string;
  readonly event_count: number;
  readonly credits_current: string;
  readonly credits_proposed: string;
  readonly credit_delta: string;
}
