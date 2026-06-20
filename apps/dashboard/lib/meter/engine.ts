//! Server-side read client for the meter engine (analytics). The dashboard reads usage time series
//! straight from the engine's authoritative data; reads degrade gracefully when the engine is down.

import type { Result } from "./client"
import type {
  AuditEntry,
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

function authHeaders(): Record<string, string> {
  const key = process.env.METER_ENGINE_API_KEY
  if (key === undefined || key.length === 0) {
    return {}
  }
  return { authorization: `Bearer ${key}` }
}

export async function fetchUsageByDay(
  accountId: string,
  start: string,
  end: string
): Promise<Result<readonly DayUsage[]>> {
  try {
    const params = new URLSearchParams({ start, end })
    const response = await fetch(
      `${ENGINE_URL}/v1/accounts/${encodeURIComponent(accountId)}/usage-by-day?${params.toString()}`,
      { cache: "no-store", headers: authHeaders() }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as DayUsage[] }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

export async function fetchUsageByModel(
  orgId: string
): Promise<Result<readonly ModelUsage[]>> {
  try {
    const response = await fetch(
      `${ENGINE_URL}/v1/orgs/${encodeURIComponent(orgId)}/usage-by-model`,
      { cache: "no-store", headers: authHeaders() }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as ModelUsage[] }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

export async function fetchEventsForAccount(
  accountId: string
): Promise<Result<readonly UsageEvent[]>> {
  try {
    const response = await fetch(
      `${ENGINE_URL}/v1/accounts/${encodeURIComponent(accountId)}/events`,
      { cache: "no-store", headers: authHeaders() }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as UsageEvent[] }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

export async function fetchAuditLog(
  limit = 100
): Promise<Result<readonly AuditEntry[]>> {
  try {
    const params = new URLSearchParams({ limit: String(limit) })
    const response = await fetch(
      `${ENGINE_URL}/v1/audit?${params.toString()}`,
      {
        cache: "no-store",
        headers: authHeaders(),
      }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as AuditEntry[] }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

export async function fetchBalance(
  accountId: string
): Promise<Result<Balance>> {
  try {
    const response = await fetch(
      `${ENGINE_URL}/v1/accounts/${encodeURIComponent(accountId)}/balance`,
      { cache: "no-store", headers: authHeaders() }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as Balance }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

export async function fetchInvoice(
  accountId: string,
  start: string,
  end: string
): Promise<Result<Invoice>> {
  try {
    const params = new URLSearchParams({ start, end })
    const response = await fetch(
      `${ENGINE_URL}/v1/accounts/${encodeURIComponent(accountId)}/invoice?${params.toString()}`,
      { cache: "no-store", headers: authHeaders() }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as Invoice }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

export async function fetchEntries(
  accountId: string
): Promise<Result<readonly LedgerEntry[]>> {
  try {
    const response = await fetch(
      `${ENGINE_URL}/v1/accounts/${encodeURIComponent(accountId)}/entries`,
      { cache: "no-store", headers: authHeaders() }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as LedgerEntry[] }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

// Amend an event's custom properties (append-only: records a corrected version). Returns the new event.
export async function amendEvent(
  eventId: string,
  properties: unknown
): Promise<Result<UsageEvent>> {
  try {
    const response = await fetch(
      `${ENGINE_URL}/v1/events/${encodeURIComponent(eventId)}/amend`,
      {
        method: "POST",
        cache: "no-store",
        headers: { "content-type": "application/json", ...authHeaders() },
        body: JSON.stringify({ properties }),
      }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as UsageEvent }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

// Void every event for a run (reverses its ledger effects). Returns the number of events voided.
export async function voidRun(runId: string): Promise<Result<number>> {
  try {
    const response = await fetch(
      `${ENGINE_URL}/v1/runs/${encodeURIComponent(runId)}/void`,
      { method: "POST", cache: "no-store", headers: authHeaders() }
    )
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    const body = (await response.json()) as { voided: number }
    return { ok: true, data: body.voided }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

// The hosted model rate-card catalog (read-only): a dated snapshot of provider-cost prices.
export async function fetchCatalog(): Promise<Result<Catalog>> {
  try {
    const response = await fetch(`${ENGINE_URL}/v1/catalog`, {
      cache: "no-store",
      headers: authHeaders(),
    })
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as Catalog }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}

// Re-rate a usage stream from one catalogued model onto another (pure projection; never the ledger).
export async function fetchSimulate(
  body: SimulateInput
): Promise<Result<SimulateResult>> {
  try {
    const response = await fetch(`${ENGINE_URL}/v1/simulate`, {
      method: "POST",
      cache: "no-store",
      headers: { "content-type": "application/json", ...authHeaders() },
      body: JSON.stringify(body),
    })
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as SimulateResult }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "engine unreachable",
    }
  }
}
