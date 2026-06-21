//! Server-side client for the meter control plane. Reads return a `Result` so a down control plane
//! degrades to a friendly empty state rather than crashing a page; mutations throw (surfaced via the
//! calling server action). Used only from Server Components and Server Actions.

import type {
  Agent,
  AlertRule,
  ApiKey,
  ApiKeyRole,
  ApiKeyScope,
  CreatedApiKey,
  Notification,
  Organization,
  Webhook,
  WebhookDelivery,
} from "./types"

import { getResult, requestOrThrow, type Result } from "./http"

export type { Result } from "./http"

const BASE_URL = process.env.METER_CONTROL_PLANE_URL ?? "http://127.0.0.1:8090"

const LABEL = "control plane"

/** Read a result's data, or a fallback when the control plane was unreachable. */
export function unwrapOr<T>(result: Result<T>, fallback: T): T {
  if (result.ok) {
    return result.data
  }
  return fallback
}

/** Bearer auth header when a control-plane API key is configured (server-side env). */
function authHeaders(): Record<string, string> {
  const key = process.env.METER_CONTROL_PLANE_API_KEY
  if (key === undefined || key.length === 0) {
    return {}
  }
  return { authorization: `Bearer ${key}` }
}

function jsonInit(method: string, body?: unknown): RequestInit {
  if (body === undefined) {
    return { method, headers: authHeaders() }
  }
  return {
    method,
    headers: { "content-type": "application/json", ...authHeaders() },
    body: JSON.stringify(body),
  }
}

function getJson<T>(path: string): Promise<Result<T>> {
  return getResult<T>(LABEL, `${BASE_URL}${path}`, { headers: authHeaders() })
}

async function post(path: string, body?: unknown): Promise<void> {
  await requestOrThrow(LABEL, `${BASE_URL}${path}`, jsonInit("POST", body))
}

async function postJson<T>(path: string, body?: unknown): Promise<T> {
  const response = await requestOrThrow(
    LABEL,
    `${BASE_URL}${path}`,
    jsonInit("POST", body)
  )
  return (await response.json()) as T
}

export function listOrganizations(): Promise<Result<readonly Organization[]>> {
  return getJson("/v1/organizations")
}

export function createOrganization(input: {
  slug: string
  name: string
}): Promise<void> {
  return post("/v1/organizations", input)
}

export function listAgents(orgId: string): Promise<Result<readonly Agent[]>> {
  return getJson(`/v1/agents?orgId=${encodeURIComponent(orgId)}`)
}

export function createAgent(input: {
  orgId: string
  key: string
  name: string
}): Promise<void> {
  return post("/v1/agents", input)
}

export function createWebhook(input: {
  orgId: string
  url: string
  secret: string
  eventTypes: readonly string[]
}): Promise<void> {
  return post("/v1/webhooks", input)
}

export interface NewAlertRuleInput {
  readonly orgId: string
  readonly name: string
  readonly scope: string
  readonly metric: string
  readonly action: string
  readonly threshold: number
  readonly accountId?: string
  readonly creditLimit?: number
  readonly windowDays?: number
}

export function createAlertRule(input: NewAlertRuleInput): Promise<void> {
  return post("/v1/alert-rules", input)
}

export function listApiKeys(orgId: string): Promise<Result<readonly ApiKey[]>> {
  return getJson(`/v1/api-keys?orgId=${encodeURIComponent(orgId)}`)
}

export function createApiKey(input: {
  orgId: string
  name: string
  role: ApiKeyRole
  scope: ApiKeyScope
}): Promise<CreatedApiKey> {
  return postJson<CreatedApiKey>("/v1/api-keys", input)
}

export function revokeApiKey(id: string): Promise<void> {
  return post(`/v1/api-keys/${encodeURIComponent(id)}/revoke`)
}

export function listNotifications(
  orgId: string,
  status?: string
): Promise<Result<readonly Notification[]>> {
  const query =
    status === undefined ? "" : `&status=${encodeURIComponent(status)}`
  return getJson(`/v1/notifications?orgId=${encodeURIComponent(orgId)}${query}`)
}

export function listAlertRules(
  orgId: string
): Promise<Result<readonly AlertRule[]>> {
  return getJson(`/v1/alert-rules?orgId=${encodeURIComponent(orgId)}`)
}

export function listWebhooks(
  orgId: string
): Promise<Result<readonly Webhook[]>> {
  return getJson(`/v1/webhooks?orgId=${encodeURIComponent(orgId)}`)
}

export function listWebhookDeliveries(
  orgId: string
): Promise<Result<readonly WebhookDelivery[]>> {
  return getJson(`/v1/webhook-deliveries?orgId=${encodeURIComponent(orgId)}`)
}

export function markNotificationRead(id: string): Promise<void> {
  return post(`/v1/notifications/${encodeURIComponent(id)}/read`)
}

export function ackNotification(id: string): Promise<void> {
  return post(`/v1/notifications/${encodeURIComponent(id)}/ack`)
}

export function setAlertRuleEnabled(
  id: string,
  enabled: boolean
): Promise<void> {
  return post(`/v1/alert-rules/${encodeURIComponent(id)}/enabled`, { enabled })
}

export function setWebhookEnabled(id: string, enabled: boolean): Promise<void> {
  return post(`/v1/webhooks/${encodeURIComponent(id)}/enabled`, { enabled })
}
