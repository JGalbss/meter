//! Server-side read client for the meter engine (analytics). The dashboard reads usage time series
//! straight from the engine's authoritative data; reads degrade gracefully when the engine is down or
//! slow (every request is timeout-bounded, see ./http).

import { getResult, type Result } from "./http"
import type {
  AuditEntry,
  AuditFilter,
  Balance,
  Catalog,
  DayUsage,
  Invoice,
  LedgerEntry,
  ModelUsage,
  SimulateInput,
  SimulateResult,
  UsageEvent,
} from "./types"

const ENGINE_URL = process.env.METER_ENGINE_URL ?? "http://127.0.0.1:8080"
const LABEL = "engine"

function authHeaders(): Record<string, string> {
  const key = process.env.METER_ENGINE_API_KEY
  if (key === undefined || key.length === 0) {
    return {}
  }
  return { authorization: `Bearer ${key}` }
}

function authInit(): RequestInit {
  return { headers: authHeaders() }
}

function postInit(body?: unknown): RequestInit {
  if (body === undefined) {
    return { method: "POST", headers: authHeaders() }
  }
  return {
    method: "POST",
    headers: { "content-type": "application/json", ...authHeaders() },
    body: JSON.stringify(body),
  }
}

function url(path: string): string {
  return `${ENGINE_URL}${path}`
}

function account(accountId: string): string {
  return `/v1/accounts/${encodeURIComponent(accountId)}`
}

export function fetchUsageByDay(
  accountId: string,
  start: string,
  end: string
): Promise<Result<readonly DayUsage[]>> {
  const params = new URLSearchParams({ start, end })
  return getResult(
    LABEL,
    url(`${account(accountId)}/usage-by-day?${params.toString()}`),
    authInit()
  )
}

export function fetchUsageByModel(
  orgId: string
): Promise<Result<readonly ModelUsage[]>> {
  return getResult(
    LABEL,
    url(`/v1/orgs/${encodeURIComponent(orgId)}/usage-by-model`),
    authInit()
  )
}

export function fetchEventsForAccount(
  accountId: string
): Promise<Result<readonly UsageEvent[]>> {
  return getResult(LABEL, url(`${account(accountId)}/events`), authInit())
}

export function fetchAuditLog(
  filter: AuditFilter = {}
): Promise<Result<readonly AuditEntry[]>> {
  const params = new URLSearchParams({ limit: String(filter.limit ?? 200) })
  const setIf = (key: string, value: string | undefined): void => {
    if (value !== undefined && value.length > 0) {
      params.set(key, value)
    }
  }
  setIf("actor", filter.actor)
  setIf("method", filter.method)
  setIf("since", filter.since)
  setIf("until", filter.until)
  return getResult(LABEL, url(`/v1/audit?${params.toString()}`), authInit())
}

export function fetchBalance(accountId: string): Promise<Result<Balance>> {
  return getResult(LABEL, url(`${account(accountId)}/balance`), authInit())
}

export function fetchInvoice(
  accountId: string,
  start: string,
  end: string
): Promise<Result<Invoice>> {
  const params = new URLSearchParams({ start, end })
  return getResult(
    LABEL,
    url(`${account(accountId)}/invoice?${params.toString()}`),
    authInit()
  )
}

export function fetchEntries(
  accountId: string
): Promise<Result<readonly LedgerEntry[]>> {
  return getResult(LABEL, url(`${account(accountId)}/entries`), authInit())
}

// Amend an event's custom properties (append-only: records a corrected version). Returns the new event.
export function amendEvent(
  eventId: string,
  properties: unknown
): Promise<Result<UsageEvent>> {
  return getResult(
    LABEL,
    url(`/v1/events/${encodeURIComponent(eventId)}/amend`),
    postInit({ properties })
  )
}

// Void every event for a run (reverses its ledger effects). Returns the number of events voided.
export async function voidRun(runId: string): Promise<Result<number>> {
  const result = await getResult<{ voided: number }>(
    LABEL,
    url(`/v1/runs/${encodeURIComponent(runId)}/void`),
    postInit()
  )
  if (!result.ok) {
    return result
  }
  return { ok: true, data: result.data.voided }
}

// The hosted model rate-card catalog (read-only): a dated snapshot of provider-cost prices.
export function fetchCatalog(): Promise<Result<Catalog>> {
  return getResult(LABEL, url("/v1/catalog"), authInit())
}

// Re-rate a usage stream from one catalogued model onto another (pure projection; never the ledger).
export function fetchSimulate(
  body: SimulateInput
): Promise<Result<SimulateResult>> {
  return getResult(LABEL, url("/v1/simulate"), postInit(body))
}
