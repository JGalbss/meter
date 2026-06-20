//! Server-side read client for the meter engine (analytics). The dashboard reads usage time series
//! straight from the engine's authoritative data; reads degrade gracefully when the engine is down.

import type { Result } from "./client";
import type { AuditEntry, DayUsage, ModelUsage } from "./types";

const ENGINE_URL = process.env.METER_ENGINE_URL ?? "http://127.0.0.1:8080";

function authHeaders(): Record<string, string> {
  const key = process.env.METER_ENGINE_API_KEY;
  if (key === undefined || key.length === 0) {
    return {};
  }
  return { authorization: `Bearer ${key}` };
}

export async function fetchUsageByDay(
  accountId: string,
  start: string,
  end: string,
): Promise<Result<readonly DayUsage[]>> {
  try {
    const params = new URLSearchParams({ start, end });
    const response = await fetch(
      `${ENGINE_URL}/v1/accounts/${encodeURIComponent(accountId)}/usage-by-day?${params.toString()}`,
      { cache: "no-store", headers: authHeaders() },
    );
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` };
    }
    return { ok: true, data: (await response.json()) as DayUsage[] };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "engine unreachable" };
  }
}

export async function fetchUsageByModel(orgId: string): Promise<Result<readonly ModelUsage[]>> {
  try {
    const response = await fetch(
      `${ENGINE_URL}/v1/orgs/${encodeURIComponent(orgId)}/usage-by-model`,
      { cache: "no-store", headers: authHeaders() },
    );
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` };
    }
    return { ok: true, data: (await response.json()) as ModelUsage[] };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "engine unreachable" };
  }
}

export async function fetchAuditLog(limit = 100): Promise<Result<readonly AuditEntry[]>> {
  try {
    const params = new URLSearchParams({ limit: String(limit) });
    const response = await fetch(`${ENGINE_URL}/v1/audit?${params.toString()}`, {
      cache: "no-store",
      headers: authHeaders(),
    });
    if (!response.ok) {
      return { ok: false, error: `engine responded ${response.status}` };
    }
    return { ok: true, data: (await response.json()) as AuditEntry[] };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "engine unreachable" };
  }
}
