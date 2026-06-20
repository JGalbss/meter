//! Server-side client for the meter control plane. Reads return a `Result` so a down control plane
//! degrades to a friendly empty state rather than crashing a page; mutations throw (surfaced via the
//! calling server action). Used only from Server Components and Server Actions.

import type {
  AlertRule,
  ApiKey,
  CreatedApiKey,
  Notification,
  Organization,
  Product,
  Webhook,
  WebhookDelivery,
} from "./types";

const BASE_URL = process.env.METER_CONTROL_PLANE_URL ?? "http://127.0.0.1:8090";

export type Result<T> = { readonly ok: true; readonly data: T } | { readonly ok: false; readonly error: string };

/** Read a result's data, or a fallback when the control plane was unreachable. */
export function unwrapOr<T>(result: Result<T>, fallback: T): T {
  if (result.ok) {
    return result.data;
  }
  return fallback;
}

function describe(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return "control plane unreachable";
}

/** Bearer auth header when a control-plane API key is configured (server-side env). */
function authHeaders(): Record<string, string> {
  const key = process.env.METER_CONTROL_PLANE_API_KEY;
  if (key === undefined || key.length === 0) {
    return {};
  }
  return { authorization: `Bearer ${key}` };
}

async function getJson<T>(path: string): Promise<Result<T>> {
  try {
    const response = await fetch(`${BASE_URL}${path}`, {
      cache: "no-store",
      headers: authHeaders(),
    });
    if (!response.ok) {
      return { ok: false, error: `control plane responded ${response.status}` };
    }
    return { ok: true, data: (await response.json()) as T };
  } catch (error) {
    return { ok: false, error: describe(error) };
  }
}

async function post(path: string, body?: unknown): Promise<void> {
  const response = await fetch(`${BASE_URL}${path}`, {
    method: "POST",
    cache: "no-store",
    headers: { "content-type": "application/json", ...authHeaders() },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`control plane responded ${response.status}`);
  }
}

async function postJson<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`${BASE_URL}${path}`, {
    method: "POST",
    cache: "no-store",
    headers: { "content-type": "application/json", ...authHeaders() },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`control plane responded ${response.status}`);
  }
  return (await response.json()) as T;
}

export function listOrganizations(): Promise<Result<readonly Organization[]>> {
  return getJson("/v1/organizations");
}

export function createOrganization(input: { slug: string; name: string }): Promise<void> {
  return post("/v1/organizations", input);
}

export function listProducts(orgId: string): Promise<Result<readonly Product[]>> {
  return getJson(`/v1/products?orgId=${encodeURIComponent(orgId)}`);
}

export function createProduct(input: {
  orgId: string;
  key: string;
  name: string;
}): Promise<void> {
  return post("/v1/products", input);
}

export function createWebhook(input: {
  orgId: string;
  url: string;
  secret: string;
  eventTypes: readonly string[];
}): Promise<void> {
  return post("/v1/webhooks", input);
}

export interface NewAlertRuleInput {
  readonly orgId: string;
  readonly name: string;
  readonly scope: string;
  readonly metric: string;
  readonly action: string;
  readonly threshold: number;
  readonly accountId?: string;
  readonly creditLimit?: number;
  readonly windowDays?: number;
}

export function createAlertRule(input: NewAlertRuleInput): Promise<void> {
  return post("/v1/alert-rules", input);
}

export function listApiKeys(orgId: string): Promise<Result<readonly ApiKey[]>> {
  return getJson(`/v1/api-keys?orgId=${encodeURIComponent(orgId)}`);
}

export function createApiKey(input: { orgId: string; name: string }): Promise<CreatedApiKey> {
  return postJson<CreatedApiKey>("/v1/api-keys", input);
}

export function revokeApiKey(id: string): Promise<void> {
  return post(`/v1/api-keys/${encodeURIComponent(id)}/revoke`);
}

export function listNotifications(
  orgId: string,
  status?: string,
): Promise<Result<readonly Notification[]>> {
  const query = status === undefined ? "" : `&status=${encodeURIComponent(status)}`;
  return getJson(`/v1/notifications?orgId=${encodeURIComponent(orgId)}${query}`);
}

export function listAlertRules(orgId: string): Promise<Result<readonly AlertRule[]>> {
  return getJson(`/v1/alert-rules?orgId=${encodeURIComponent(orgId)}`);
}

export function listWebhooks(orgId: string): Promise<Result<readonly Webhook[]>> {
  return getJson(`/v1/webhooks?orgId=${encodeURIComponent(orgId)}`);
}

export function listWebhookDeliveries(orgId: string): Promise<Result<readonly WebhookDelivery[]>> {
  return getJson(`/v1/webhook-deliveries?orgId=${encodeURIComponent(orgId)}`);
}

export function markNotificationRead(id: string): Promise<void> {
  return post(`/v1/notifications/${encodeURIComponent(id)}/read`);
}

export function ackNotification(id: string): Promise<void> {
  return post(`/v1/notifications/${encodeURIComponent(id)}/ack`);
}

export function setAlertRuleEnabled(id: string, enabled: boolean): Promise<void> {
  return post(`/v1/alert-rules/${encodeURIComponent(id)}/enabled`, { enabled });
}

export interface EvaluationSummary {
  readonly evaluated: number;
  readonly raised: number;
}

export async function evaluateAlertRules(orgId: string): Promise<EvaluationSummary> {
  const response = await fetch(
    `${BASE_URL}/v1/alert-rules/evaluate?orgId=${encodeURIComponent(orgId)}`,
    { method: "POST", cache: "no-store", headers: authHeaders() },
  );
  if (!response.ok) {
    throw new Error(`control plane responded ${response.status}`);
  }
  return (await response.json()) as EvaluationSummary;
}

export function setWebhookEnabled(id: string, enabled: boolean): Promise<void> {
  return post(`/v1/webhooks/${encodeURIComponent(id)}/enabled`, { enabled });
}
