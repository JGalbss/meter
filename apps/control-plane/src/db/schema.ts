//! Control-plane configuration schema (Drizzle). This is the config side of the system — distinct
//! from the engine's ledger/event schema. The engine owns money; the control plane owns config.

import { pgTable, text, timestamp, unique, uuid } from "drizzle-orm/pg-core";

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
