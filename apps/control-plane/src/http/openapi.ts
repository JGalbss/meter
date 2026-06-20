//! OpenAPI 3.1 document for the control-plane API.
//!
//! Request bodies are derived from the **same** Effect `Schema`s the routes validate with (via
//! `JSONSchema.make`), so the contract can never drift from validation and nothing is hand-mirrored.
//! Query/path params are plain strings. Response bodies are described as JSON for now; typing them is
//! the documented next step (it needs the repositories' response interfaces moved to `Schema` as their
//! single source — tracked in `tickets`). Served at `GET /openapi.json` and consumed by client codegen.

import { JSONSchema, type Schema } from "effect";

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
}

const ORG_QUERY = [{ name: "orgId", required: true }] as const;

const OPERATIONS: ReadonlyArray<Operation> = [
  { method: "get", path: "/health", tag: "Health", summary: "Liveness probe" },

  { method: "get", path: "/v1/organizations", tag: "Organizations", summary: "List organizations" },
  {
    method: "post",
    path: "/v1/organizations",
    tag: "Organizations",
    summary: "Create an organization",
    body: NewOrganizationBody,
  },

  {
    method: "get",
    path: "/v1/products",
    tag: "Products",
    summary: "List products",
    query: ORG_QUERY,
  },
  {
    method: "post",
    path: "/v1/products",
    tag: "Products",
    summary: "Create a product",
    body: NewProductBody,
  },

  {
    method: "get",
    path: "/v1/api-keys",
    tag: "API keys",
    summary: "List API keys",
    query: ORG_QUERY,
  },
  {
    method: "post",
    path: "/v1/api-keys",
    tag: "API keys",
    summary: "Mint an API key (token shown once)",
    body: NewApiKeyBody,
  },
  {
    method: "post",
    path: "/v1/api-keys/{id}/revoke",
    tag: "API keys",
    summary: "Revoke an API key",
    pathParams: ["id"],
  },

  {
    method: "get",
    path: "/v1/alert-rules",
    tag: "Alert rules",
    summary: "List alert rules",
    query: ORG_QUERY,
  },
  {
    method: "post",
    path: "/v1/alert-rules",
    tag: "Alert rules",
    summary: "Create an alert rule",
    body: NewAlertRuleBody,
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
  },
  {
    method: "post",
    path: "/v1/notifications",
    tag: "Notifications",
    summary: "Raise a notification",
    body: NewNotificationBody,
  },
  {
    method: "post",
    path: "/v1/notifications/{id}/read",
    tag: "Notifications",
    summary: "Mark read",
    pathParams: ["id"],
  },
  {
    method: "post",
    path: "/v1/notifications/{id}/ack",
    tag: "Notifications",
    summary: "Acknowledge",
    pathParams: ["id"],
  },

  {
    method: "get",
    path: "/v1/webhooks",
    tag: "Webhooks",
    summary: "List webhooks",
    query: ORG_QUERY,
  },
  {
    method: "post",
    path: "/v1/webhooks",
    tag: "Webhooks",
    summary: "Register a webhook",
    body: NewWebhookBody,
  },
  {
    method: "post",
    path: "/v1/webhooks/{id}/enabled",
    tag: "Webhooks",
    summary: "Enable/disable a webhook",
    pathParams: ["id"],
    body: EnabledBody,
  },
  {
    method: "get",
    path: "/v1/webhook-deliveries",
    tag: "Webhooks",
    summary: "List webhook deliveries",
    query: ORG_QUERY,
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
  const operation: Record<string, unknown> = {
    tags: [op.tag],
    summary: op.summary,
    operationId: `${op.method}:${op.path}`,
    responses: {
      "200": {
        description: "Success",
        content: { "application/json": { schema: { type: "object" } } },
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
