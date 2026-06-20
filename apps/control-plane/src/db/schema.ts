//! Control-plane configuration schema (Drizzle). This is the config side of the system — distinct
//! from the engine's ledger/event schema. The engine owns money; the control plane owns config.

import {
  boolean,
  index,
  integer,
  jsonb,
  numeric,
  pgTable,
  text,
  timestamp,
  unique,
  uuid,
} from "drizzle-orm/pg-core";

export const organizations = pgTable("organizations", {
  id: uuid("id").primaryKey().defaultRandom(),
  slug: text("slug").notNull().unique(),
  name: text("name").notNull(),
  defaultCurrency: text("default_currency").notNull().default("USD"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
});

export const products = pgTable(
  "products",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    orgId: uuid("org_id")
      .notNull()
      .references(() => organizations.id),
    key: text("key").notNull(),
    name: text("name").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => ({
    orgKey: unique("products_org_key").on(table.orgId, table.key),
  }),
);

// Alert rules: thresholds that, when crossed, raise notifications (and later drive webhooks/enforce).
// `metric` (budget|credit|spend) is evaluated against `threshold` per `scope` (org|team|user|product).
export const alertRules = pgTable(
  "alert_rules",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    orgId: uuid("org_id")
      .notNull()
      .references(() => organizations.id),
    name: text("name").notNull(),
    scope: text("scope").notNull(),
    metric: text("metric").notNull(),
    threshold: numeric("threshold").notNull(),
    action: text("action").notNull(),
    enabled: boolean("enabled").notNull().default(true),
    // Budget evaluation: the engine account to watch and its credit cap, evaluated over a rolling
    // window. `lastStatus` records the last classification so we only alert on escalation.
    accountId: uuid("account_id"),
    creditLimit: numeric("credit_limit"),
    windowDays: integer("window_days").notNull().default(30),
    lastStatus: text("last_status"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => ({
    byOrg: index("alert_rules_org").on(table.orgId),
  }),
);

// Notifications: first-class, pullable records. A principal lists them, marks them read, and acks them.
export const notifications = pgTable(
  "notifications",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    orgId: uuid("org_id")
      .notNull()
      .references(() => organizations.id),
    type: text("type").notNull(),
    severity: text("severity").notNull(),
    title: text("title").notNull(),
    body: text("body").notNull().default(""),
    data: jsonb("data").notNull().default({}),
    status: text("status").notNull().default("unread"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    readAt: timestamp("read_at", { withTimezone: true }),
    ackedAt: timestamp("acked_at", { withTimezone: true }),
  },
  (table) => ({
    byOrgStatus: index("notifications_org_status").on(table.orgId, table.status),
  }),
);

// API keys: bearer tokens that authenticate control-plane requests. Only a SHA-256 hash of the token
// is stored; the plaintext is shown once at creation. `prefix` is a non-secret display fragment.
export const apiKeys = pgTable(
  "api_keys",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    orgId: uuid("org_id")
      .notNull()
      .references(() => organizations.id),
    name: text("name").notNull(),
    // RBAC role: "viewer" (read-only), "member" (writes), or "admin" (manage keys/webhooks).
    // Defaults to "admin" so pre-RBAC keys retain full access.
    role: text("role").notNull().default("admin"),
    prefix: text("prefix").notNull(),
    tokenHash: text("token_hash").notNull().unique(),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    lastUsedAt: timestamp("last_used_at", { withTimezone: true }),
    revokedAt: timestamp("revoked_at", { withTimezone: true }),
  },
  (table) => ({
    byOrg: index("api_keys_org").on(table.orgId),
  }),
);

// Webhooks: signed HTTP callbacks. `eventTypes` is a list of notification types this endpoint wants;
// an empty list means all types. `secret` keys the HMAC-SHA256 signature on every delivery.
export const webhooks = pgTable(
  "webhooks",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    orgId: uuid("org_id")
      .notNull()
      .references(() => organizations.id),
    url: text("url").notNull(),
    secret: text("secret").notNull(),
    eventTypes: jsonb("event_types").$type<string[]>().notNull().default([]),
    enabled: boolean("enabled").notNull().default(true),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => ({
    byOrg: index("webhooks_org").on(table.orgId),
  }),
);

// Webhook deliveries: an append-only record of every delivery attempt — the audit trail and the
// dead-letter view for endpoints that ultimately failed.
export const webhookDeliveries = pgTable(
  "webhook_deliveries",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    webhookId: uuid("webhook_id")
      .notNull()
      .references(() => webhooks.id),
    notificationId: uuid("notification_id"),
    event: text("event").notNull(),
    payload: jsonb("payload").notNull(),
    status: text("status").notNull(),
    responseStatus: integer("response_status"),
    error: text("error"),
    attempts: integer("attempts").notNull().default(0),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => ({
    byWebhook: index("webhook_deliveries_webhook").on(table.webhookId),
  }),
);
