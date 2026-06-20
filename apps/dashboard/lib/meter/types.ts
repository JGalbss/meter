//! Response types the dashboard consumes.
//!
//! Control-plane types are **generated** from its OpenAPI spec (`bun run gen:api` →
//! `control-plane.gen.ts`), so they can't drift from the API. Engine read-models are still hand-written
//! (the engine doesn't emit OpenAPI yet).

import type { components } from "./control-plane.gen"

// --- Control plane (generated) -------------------------------------------------------------------
type CpSchemas = components["schemas"]

export type Organization = CpSchemas["Organization"]
export type Product = CpSchemas["Product"]
export type Notification = CpSchemas["Notification"]
export type AlertRule = CpSchemas["AlertRule"]
export type Webhook = CpSchemas["Webhook"]
export type WebhookDelivery = CpSchemas["WebhookDelivery"]
export type ApiKey = CpSchemas["ApiKey"]
export type CreatedApiKey = CpSchemas["CreatedApiKey"]
export type ApiKeyRole = ApiKey["role"]

// --- Engine read-models (hand-written until the engine emits OpenAPI) -----------------------------
export interface DayUsage {
  readonly day: string
  readonly total_credits: string
  readonly entry_count: number
}

export interface ModelUsage {
  readonly model: string
  readonly events: number
  readonly input_tokens: number
  readonly output_tokens: number
  readonly credits: number
}

export interface AuditEntry {
  readonly id: string
  readonly actor: string
  readonly method: string
  readonly path: string
  readonly status: number
  readonly created_at: string
}

export type EventStatus = "recorded" | "amended" | "voided"

export interface UsageEvent {
  readonly id: string
  readonly org_id: string
  readonly idempotency_key: string
  readonly event_time: string
  readonly meter: string
  readonly account_id: string
  readonly run_id: string | null
  readonly properties: unknown
  readonly status: EventStatus
  readonly supersedes: string | null
  readonly created_at: string
}

export interface Balance {
  readonly settled: string
  readonly held: string
}

export interface Invoice {
  readonly account_id: string
  readonly total_credits: string
  readonly entries: number
}

export type EntryType =
  | "grant"
  | "usage"
  | "reservation_hold"
  | "settle"
  | "partial_return"
  | "transfer"
  | "void"
  | "refund"
  | "chargeback"
  | "expiration"
  | "amendment"
  | "fx"
  | "sealing"

export interface LedgerEntry {
  readonly id: string
  readonly account_id: string
  readonly paired_account_id: string
  readonly entry_type: EntryType
  readonly delta_credits: string
  readonly balance_after: string
  readonly source: string | null
  readonly revenue_recognizable: boolean
  readonly reverses_entry_id: string | null
  readonly reservation_id: string | null
  readonly idempotency_key: string | null
  readonly created_at: string
}

/** One model's provider-cost prices from the engine's hosted catalog. Decimals are exact strings. */
export interface RateCardEntry {
  readonly provider: string
  readonly model_id: string
  readonly input_per_token: string
  readonly cache_read_per_token: string
  readonly cache_write_per_token: string
  readonly output_per_token: string
}

/** The engine's `GET /v1/catalog` response: a dated snapshot of model rate cards. */
export interface Catalog {
  readonly as_of: string
  readonly models: readonly RateCardEntry[]
}

/** Token counts for one usage event, re-rated by the pricing simulator. */
export interface SimulateUsage {
  readonly input_uncached: number
  readonly cache_read: number
  readonly cache_write: number
  readonly output: number
  readonly reasoning: number
}

/** `POST /v1/simulate` request: re-rate a usage stream from one catalogued model onto another. */
export interface SimulateInput {
  readonly current_model: string
  readonly proposed_model: string
  readonly events: readonly SimulateUsage[]
}

/** `POST /v1/simulate` response: credits under each model and the delta (exact decimal strings). */
export interface SimulateResult {
  readonly current_model: string
  readonly proposed_model: string
  readonly event_count: number
  readonly credits_current: string
  readonly credits_proposed: string
  readonly credit_delta: string
}
