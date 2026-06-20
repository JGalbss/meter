//! Response shapes from the meter control plane. Mirrors `apps/control-plane` repositories.

export interface Organization {
  readonly id: string
  readonly slug: string
  readonly name: string
  readonly defaultCurrency: string
}

export interface Product {
  readonly id: string
  readonly orgId: string
  readonly key: string
  readonly name: string
}

export interface Notification {
  readonly id: string
  readonly orgId: string
  readonly type: string
  readonly severity: string
  readonly title: string
  readonly body: string
  readonly data: unknown
  readonly status: string
  readonly createdAt: string
  readonly readAt: string | null
  readonly ackedAt: string | null
}

export interface AlertRule {
  readonly id: string
  readonly orgId: string
  readonly name: string
  readonly scope: string
  readonly metric: string
  readonly threshold: string
  readonly action: string
  readonly enabled: boolean
  readonly accountId: string | null
  readonly creditLimit: string | null
  readonly windowDays: number
  readonly lastStatus: string | null
  readonly createdAt: string
}

export interface Webhook {
  readonly id: string
  readonly orgId: string
  readonly url: string
  readonly eventTypes: readonly string[]
  readonly enabled: boolean
  readonly createdAt: string
}

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

export type ApiKeyRole = "viewer" | "member" | "admin"

export interface ApiKey {
  readonly id: string
  readonly orgId: string
  readonly name: string
  readonly role: ApiKeyRole
  readonly prefix: string
  readonly createdAt: string
  readonly lastUsedAt: string | null
  readonly revokedAt: string | null
}

export interface CreatedApiKey extends ApiKey {
  readonly token: string
}

export interface WebhookDelivery {
  readonly id: string
  readonly webhookId: string
  readonly notificationId: string | null
  readonly event: string
  readonly payload: unknown
  readonly status: string
  readonly responseStatus: number | null
  readonly error: string | null
  readonly attempts: number
  readonly createdAt: string
}
