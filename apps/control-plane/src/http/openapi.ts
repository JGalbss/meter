//! OpenAPI 3.1 document for the control-plane API.
//!
//! Request bodies are derived from the **same** Effect `Schema`s the routes validate with (via
//! `JSONSchema.make`), so the contract can never drift from validation and nothing is hand-mirrored.
//! Query/path params are plain strings. Response bodies are described as JSON for now; typing them is
//! the documented next step (it needs the repositories' response interfaces moved to `Schema` as their
//! single source — tracked in `tickets`). Served at `GET /openapi.json` and consumed by client codegen.

import { JSONSchema, type Schema } from "effect";

import { AlertRule } from "../alerts/repository";
import { ApiKey, CreatedApiKey } from "../api-keys/repository";
import { Notification } from "../notifications/repository";
import { Organization } from "../organizations/repository";
import { Product } from "../products/repository";
import { Webhook, WebhookDelivery } from "../webhooks/repository";
import { NewAlertRuleBody } from "./routes/alerts";
import { NewApiKeyBody } from "./routes/api-keys";
import { NewNotificationBody } from "./routes/notifications";
import { NewOrganizationBody } from "./routes/organizations";
import { NewProductBody } from "./routes/products";
import { EnabledBody, NewWebhookBody } from "./routes/webhooks";

type Method = "get" | "post";

interface Operation {
  readonly method: Method;
  readonly path: string;
  readonly tag: string;
  readonly summary: string;
  readonly body?: Schema.Schema.AnyNoContext;
  readonly query?: ReadonlyArray<{ name: string; required: boolean }>;
  readonly pathParams?: ReadonlyArray<string>;
  // Success response body, derived from the same repository Schema. `array` for list endpoints.
  readonly response?: { readonly schema: Schema.Schema.AnyNoContext; readonly array?: boolean };
  // Create endpoints answer 201 Created rather than 200.
  readonly created?: boolean;
}

const ORG_QUERY = [{ name: "orgId", required: true }] as const;

const OPERATIONS: ReadonlyArray<Operation> = [
  { method: "get", path: "/health", tag: "Health", summary: "Liveness probe" },

  {
    method: "get",
    path: "/v1/organizations",
    tag: "Organizations",
    summary: "List organizations",
    response: { schema: Organization, array: true },
  },
  {
    method: "post",
    path: "/v1/organizations",
    tag: "Organizations",
    summary: "Create an organization",
    body: NewOrganizationBody,
    response: { schema: Organization },
    created: true,
  },

  {
    method: "get",
    path: "/v1/products",
    tag: "Products",
    summary: "List products",
    query: ORG_QUERY,
    response: { schema: Product, array: true },
  },
  {
    method: "post",
    path: "/v1/products",
    tag: "Products",
    summary: "Create a product",
    body: NewProductBody,
    response: { schema: Product },
    created: true,
  },

  {
    method: "get",
    path: "/v1/api-keys",
    tag: "API keys",
    summary: "List API keys",
    query: ORG_QUERY,
    response: { schema: ApiKey, array: true },
  },
  {
    method: "post",
    path: "/v1/api-keys",
    tag: "API keys",
    summary: "Mint an API key (token shown once)",
    body: NewApiKeyBody,
    response: { schema: CreatedApiKey },
    created: true,
  },
  {
    method: "post",
    path: "/v1/api-keys/{id}/revoke",
    tag: "API keys",
    summary: "Revoke an API key",
    pathParams: ["id"],
    response: { schema: ApiKey },
  },

  {
    method: "get",
    path: "/v1/alert-rules",
    tag: "Alert rules",
    summary: "List alert rules",
    query: ORG_QUERY,
    response: { schema: AlertRule, array: true },
  },
  {
    method: "post",
    path: "/v1/alert-rules",
    tag: "Alert rules",
    summary: "Create an alert rule",
    body: NewAlertRuleBody,
    response: { schema: AlertRule },
    created: true,
  },
  {
    method: "post",
    path: "/v1/alert-rules/evaluate",
    tag: "Alert rules",
    summary: "Evaluate budget rules now",
    query: ORG_QUERY,
  },
  {
    method: "post",
    path: "/v1/alert-rules/{id}/enabled",
    tag: "Alert rules",
    summary: "Enable/disable a rule",
    pathParams: ["id"],
    body: EnabledBody,
    response: { schema: AlertRule },
  },

  {
    method: "get",
    path: "/v1/notifications",
    tag: "Notifications",
    summary: "List notifications",
    query: [
      { name: "orgId", required: true },
      { name: "status", required: false },
    ],
    response: { schema: Notification, array: true },
  },
  {
    method: "post",
    path: "/v1/notifications",
    tag: "Notifications",
    summary: "Raise a notification",
    body: NewNotificationBody,
    response: { schema: Notification },
    created: true,
  },
  {
    method: "post",
    path: "/v1/notifications/{id}/read",
    tag: "Notifications",
    summary: "Mark read",
    pathParams: ["id"],
    response: { schema: Notification },
  },
  {
    method: "post",
    path: "/v1/notifications/{id}/ack",
    tag: "Notifications",
    summary: "Acknowledge",
    pathParams: ["id"],
    response: { schema: Notification },
  },

  {
    method: "get",
    path: "/v1/webhooks",
    tag: "Webhooks",
    summary: "List webhooks",
    query: ORG_QUERY,
    response: { schema: Webhook, array: true },
  },
  {
    method: "post",
    path: "/v1/webhooks",
    tag: "Webhooks",
    summary: "Register a webhook",
    body: NewWebhookBody,
    response: { schema: Webhook },
    created: true,
  },
  {
    method: "post",
    path: "/v1/webhooks/{id}/enabled",
    tag: "Webhooks",
    summary: "Enable/disable a webhook",
    pathParams: ["id"],
    body: EnabledBody,
    response: { schema: Webhook },
  },
  {
    method: "get",
    path: "/v1/webhook-deliveries",
    tag: "Webhooks",
    summary: "List webhook deliveries",
    query: ORG_QUERY,
    response: { schema: WebhookDelivery, array: true },
  },
];

function bodySchema(schema: Schema.Schema.AnyNoContext): Record<string, unknown> {
  // JSONSchema.make emits a `$schema` meta key; drop it so the value is a clean inline JSON Schema.
  const full = JSONSchema.make(schema) as unknown as Record<string, unknown>;
  const { $schema, ...rest } = full;
  void $schema;
  return rest;
}

function operationObject(op: Operation): Record<string, unknown> {
  const parameters = [
    ...(op.pathParams ?? []).map((name) => ({
      name,
      in: "path",
      required: true,
      schema: { type: "string" },
    })),
    ...(op.query ?? []).map((q) => ({
      name: q.name,
      in: "query",
      required: q.required,
      schema: { type: "string" },
    })),
  ];
  const okSchema = ((): Record<string, unknown> => {
    if (op.response === undefined) {
      return { type: "object" };
    }
    const item = bodySchema(op.response.schema);
    if (op.response.array === true) {
      return { type: "array", items: item };
    }
    return item;
  })();
  const successStatus = op.created === true ? "201" : "200";
  const operation: Record<string, unknown> = {
    tags: [op.tag],
    summary: op.summary,
    operationId: `${op.method}:${op.path}`,
    responses: {
      [successStatus]: {
        description: op.created === true ? "Created" : "Success",
        content: { "application/json": { schema: okSchema } },
      },
      "400": { description: "Invalid request" },
      "401": { description: "Unauthorized" },
      "403": { description: "Forbidden (insufficient role)" },
      "404": { description: "Not found" },
    },
  };
  if (parameters.length > 0) {
    operation.parameters = parameters;
  }
  if (op.body !== undefined) {
    operation.requestBody = {
      required: true,
      content: { "application/json": { schema: bodySchema(op.body) } },
    };
  }
  return operation;
}

function buildPaths(): Record<string, Record<string, unknown>> {
  const paths: Record<string, Record<string, unknown>> = {};
  for (const op of OPERATIONS) {
    const entry = paths[op.path] ?? {};
    entry[op.method] = operationObject(op);
    paths[op.path] = entry;
  }
  return paths;
}

/** The control-plane OpenAPI 3.1 document. */
export const openApiDocument: Record<string, unknown> = {
  openapi: "3.1.0",
  info: {
    title: "meter control plane",
    version: "0.0.0",
    description:
      "Configuration and operations API for meter (organizations, products, API keys, alert rules, notifications, webhooks). Computes no money — the engine owns money-truth.",
  },
  servers: [{ url: "/" }],
  security: [{ bearerAuth: [] }],
  components: {
    securitySchemes: {
      bearerAuth: { type: "http", scheme: "bearer", description: "API key as a bearer token." },
    },
  },
  tags: [
    { name: "Organizations" },
    { name: "Products" },
    { name: "API keys" },
    { name: "Alert rules" },
    { name: "Notifications" },
    { name: "Webhooks" },
    { name: "Health" },
  ],
  paths: buildPaths(),
};
