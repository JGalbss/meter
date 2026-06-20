//! Control-plane configuration schema (Drizzle). This is the config side of the system — distinct
//! from the engine's ledger/event schema. The engine owns money; the control plane owns config.

import {
  boolean,
  index,
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
