//! Alert-rules repository — thresholds that raise notifications (and later drive webhooks/enforce).
//! Effect-wrapped Drizzle queries with typed error channels.

import { desc, eq } from "drizzle-orm";
import { Effect } from "effect";

import type { Db } from "../db/client";
import { alertRules } from "../db/schema";
import { NotFound, RepoError } from "../repository/errors";

export interface AlertRule {
  readonly id: string;
  readonly orgId: string;
  readonly name: string;
  readonly scope: string;
  readonly metric: string;
  readonly threshold: string;
  readonly action: string;
  readonly enabled: boolean;
  readonly createdAt: string;
}

export interface NewAlertRule {
  readonly orgId: string;
  readonly name: string;
  readonly scope: string;
  readonly metric: string;
  readonly threshold: string;
  readonly action: string;
  readonly enabled?: boolean | undefined;
}

function toAlertRule(row: typeof alertRules.$inferSelect): AlertRule {
  return {
    id: row.id,
    orgId: row.orgId,
    name: row.name,
    scope: row.scope,
    metric: row.metric,
    threshold: row.threshold,
    action: row.action,
    enabled: row.enabled,
    createdAt: row.createdAt.toISOString(),
  };
}

function requireRow<A>(row: A | undefined, id: string): Effect.Effect<A, NotFound> {
  if (row === undefined) {
    return Effect.fail(new NotFound({ resource: "alert_rule", id }));
  }
  return Effect.succeed(row);
}

/** Create an alert rule. */
export function createAlertRule(db: Db, input: NewAlertRule): Effect.Effect<AlertRule, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db
        .insert(alertRules)
        .values({
          orgId: input.orgId,
          name: input.name,
          scope: input.scope,
          metric: input.metric,
          threshold: input.threshold,
          action: input.action,
          enabled: input.enabled ?? true,
        })
        .returning();
      if (row === undefined) {
        throw new Error("insert returned no row");
      }
      return toAlertRule(row);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** List an organization's alert rules, newest first. */
export function listAlertRules(
  db: Db,
  orgId: string,
): Effect.Effect<readonly AlertRule[], RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const rows = await db
        .select()
        .from(alertRules)
        .where(eq(alertRules.orgId, orgId))
        .orderBy(desc(alertRules.createdAt));
      return rows.map(toAlertRule);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** Enable or disable an alert rule. */
export function setAlertRuleEnabled(
  db: Db,
  id: string,
  enabled: boolean,
): Effect.Effect<AlertRule, RepoError | NotFound> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db
        .update(alertRules)
        .set({ enabled })
        .where(eq(alertRules.id, id))
        .returning();
      return row;
    },
    catch: (cause) => new RepoError({ cause }),
  }).pipe(
    Effect.flatMap((row) => requireRow(row, id)),
    Effect.map(toAlertRule),
  );
}
