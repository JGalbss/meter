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
