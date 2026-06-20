//! Notifications repository — first-class, pullable records. A principal lists them, marks them read,
//! and acknowledges them. Effect-wrapped Drizzle queries with typed error channels.

import { type SQL, and, desc, eq } from "drizzle-orm";
import { Effect } from "effect";

import type { Db } from "../db/client";
import { notifications } from "../db/schema";
import { NotFound, RepoError } from "../repository/errors";

export interface Notification {
  readonly id: string;
  readonly orgId: string;
  readonly type: string;
  readonly severity: string;
  readonly title: string;
  readonly body: string;
  readonly data: unknown;
  readonly status: string;
  readonly createdAt: string;
  readonly readAt: string | null;
  readonly ackedAt: string | null;
}

export interface NewNotification {
  readonly orgId: string;
  readonly type: string;
  readonly severity: string;
  readonly title: string;
  readonly body?: string | undefined;
  readonly data?: unknown;
}

function isoOrNull(at: Date | null): string | null {
  if (at === null) {
    return null;
  }
  return at.toISOString();
}

function toNotification(row: typeof notifications.$inferSelect): Notification {
  return {
    id: row.id,
    orgId: row.orgId,
    type: row.type,
    severity: row.severity,
    title: row.title,
    body: row.body,
    data: row.data,
    status: row.status,
    createdAt: row.createdAt.toISOString(),
    readAt: isoOrNull(row.readAt),
    ackedAt: isoOrNull(row.ackedAt),
  };
}

function requireRow<A>(row: A | undefined, id: string): Effect.Effect<A, NotFound> {
  if (row === undefined) {
    return Effect.fail(new NotFound({ resource: "notification", id }));
  }
  return Effect.succeed(row);
}

function scopeFilter(orgId: string, status: string | undefined): SQL | undefined {
  if (status === undefined) {
    return eq(notifications.orgId, orgId);
  }
  return and(eq(notifications.orgId, orgId), eq(notifications.status, status));
}

/** Raise a notification. */
export function createNotification(
  db: Db,
  input: NewNotification,
): Effect.Effect<Notification, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db
        .insert(notifications)
        .values({
          orgId: input.orgId,
          type: input.type,
          severity: input.severity,
          title: input.title,
          body: input.body ?? "",
          data: input.data ?? {},
        })
        .returning();
      if (row === undefined) {
        throw new Error("insert returned no row");
      }
      return toNotification(row);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** List an organization's notifications, newest first, optionally filtered by status. */
export function listNotifications(
  db: Db,
  orgId: string,
  status?: string,
): Effect.Effect<readonly Notification[], RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const rows = await db
        .select()
        .from(notifications)
        .where(scopeFilter(orgId, status))
        .orderBy(desc(notifications.createdAt));
      return rows.map(toNotification);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

function updateStatus(
  db: Db,
  id: string,
  values: Partial<typeof notifications.$inferInsert>,
): Effect.Effect<Notification, RepoError | NotFound> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db
        .update(notifications)
        .set(values)
        .where(eq(notifications.id, id))
        .returning();
      return row;
    },
    catch: (cause) => new RepoError({ cause }),
  }).pipe(
    Effect.flatMap((row) => requireRow(row, id)),
    Effect.map(toNotification),
  );
}

/** Mark a notification read. */
export function markNotificationRead(
  db: Db,
  id: string,
  now: Date,
): Effect.Effect<Notification, RepoError | NotFound> {
  return updateStatus(db, id, { status: "read", readAt: now });
}

/** Acknowledge a notification (also marks it read if it was not already). */
export function ackNotification(
  db: Db,
  id: string,
  now: Date,
): Effect.Effect<Notification, RepoError | NotFound> {
  return updateStatus(db, id, { status: "acked", readAt: now, ackedAt: now });
}
