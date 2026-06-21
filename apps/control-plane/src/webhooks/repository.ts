//! Webhooks repository — endpoint config plus the append-only delivery log. Effect-wrapped Drizzle
//! queries with typed error channels.

import { desc, eq } from "drizzle-orm";
import { Effect, Schema } from "effect";

import type { Db } from "../db/client";
import { webhookDeliveries, webhooks } from "../db/schema";
import { NotFound, RepoError } from "../repository/errors";
import { byIdInOrg } from "../repository/scope";

// The response Schema is the single source of truth for the `Webhook` type + the OpenAPI contract.
export const Webhook = Schema.Struct({
  id: Schema.String,
  orgId: Schema.String,
  url: Schema.String,
  eventTypes: Schema.Array(Schema.String),
  enabled: Schema.Boolean,
  createdAt: Schema.String,
});
export type Webhook = typeof Webhook.Type;

export interface NewWebhook {
  readonly orgId: string;
  readonly url: string;
  readonly secret: string;
  readonly eventTypes?: readonly string[] | undefined;
}

// The response Schema is the single source of truth for `WebhookDelivery` + the OpenAPI contract.
export const WebhookDelivery = Schema.Struct({
  id: Schema.String,
  webhookId: Schema.String,
  notificationId: Schema.NullOr(Schema.String),
  event: Schema.String,
  payload: Schema.Unknown,
  status: Schema.String,
  responseStatus: Schema.NullOr(Schema.Number),
  error: Schema.NullOr(Schema.String),
  attempts: Schema.Number,
  createdAt: Schema.String,
});
export type WebhookDelivery = typeof WebhookDelivery.Type;

export interface RecordDelivery {
  readonly webhookId: string;
  readonly notificationId: string | null;
  readonly event: string;
  readonly payload: unknown;
  readonly status: string;
  readonly responseStatus: number | null;
  readonly error: string | null;
  readonly attempts: number;
}

/** A webhook with its secret — used only internally for signing, never returned over the API. */
export type WebhookSecretRow = typeof webhooks.$inferSelect;

function toWebhook(row: typeof webhooks.$inferSelect): Webhook {
  return {
    id: row.id,
    orgId: row.orgId,
    url: row.url,
    eventTypes: row.eventTypes,
    enabled: row.enabled,
    createdAt: row.createdAt.toISOString(),
  };
}

function toDelivery(row: typeof webhookDeliveries.$inferSelect): WebhookDelivery {
  return {
    id: row.id,
    webhookId: row.webhookId,
    notificationId: row.notificationId,
    event: row.event,
    payload: row.payload,
    status: row.status,
    responseStatus: row.responseStatus,
    error: row.error,
    attempts: row.attempts,
    createdAt: row.createdAt.toISOString(),
  };
}

function requireRow<A>(row: A | undefined, id: string): Effect.Effect<A, NotFound> {
  if (row === undefined) {
    return Effect.fail(new NotFound({ resource: "webhook", id }));
  }
  return Effect.succeed(row);
}

/** Register a webhook endpoint (secret is stored but never returned). */
export function createWebhook(db: Db, input: NewWebhook): Effect.Effect<Webhook, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db
        .insert(webhooks)
        .values({
          orgId: input.orgId,
          url: input.url,
          secret: input.secret,
          eventTypes: [...(input.eventTypes ?? [])],
        })
        .returning();
      if (row === undefined) {
        throw new Error("insert returned no row");
      }
      return toWebhook(row);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** List an organization's webhooks, newest first. */
export function listWebhooks(db: Db, orgId: string): Effect.Effect<readonly Webhook[], RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const rows = await db
        .select()
        .from(webhooks)
        .where(eq(webhooks.orgId, orgId))
        .orderBy(desc(webhooks.createdAt));
      return rows.map(toWebhook);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** The enabled webhook rows (with secrets) for an org — for the dispatcher. */
export function enabledWebhooks(
  db: Db,
  orgId: string,
): Effect.Effect<readonly WebhookSecretRow[], RepoError> {
  return Effect.tryPromise({
    try: () => db.select().from(webhooks).where(eq(webhooks.orgId, orgId)),
    catch: (cause) => new RepoError({ cause }),
  }).pipe(Effect.map((rows) => rows.filter((row) => row.enabled)));
}

/** Enable or disable a webhook. */
export function setWebhookEnabled(
  db: Db,
  id: string,
  orgId: string | null,
  enabled: boolean,
): Effect.Effect<Webhook, RepoError | NotFound> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db
        .update(webhooks)
        .set({ enabled })
        .where(byIdInOrg(webhooks.id, webhooks.orgId, id, orgId))
        .returning();
      return row;
    },
    catch: (cause) => new RepoError({ cause }),
  }).pipe(
    Effect.flatMap((row) => requireRow(row, id)),
    Effect.map(toWebhook),
  );
}

/** Record a delivery attempt outcome. */
export function recordDelivery(
  db: Db,
  delivery: RecordDelivery,
): Effect.Effect<WebhookDelivery, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db.insert(webhookDeliveries).values(delivery).returning();
      if (row === undefined) {
        throw new Error("insert returned no row");
      }
      return toDelivery(row);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** List an organization's delivery log (audit + dead-letter), newest first. */
export function listDeliveries(
  db: Db,
  orgId: string,
): Effect.Effect<readonly WebhookDelivery[], RepoError> {
  return Effect.tryPromise({
    try: () =>
      db
        .select({ delivery: webhookDeliveries })
        .from(webhookDeliveries)
        .innerJoin(webhooks, eq(webhookDeliveries.webhookId, webhooks.id))
        .where(eq(webhooks.orgId, orgId))
        .orderBy(desc(webhookDeliveries.createdAt)),
    catch: (cause) => new RepoError({ cause }),
  }).pipe(Effect.map((rows) => rows.map((row) => toDelivery(row.delivery))));
}
