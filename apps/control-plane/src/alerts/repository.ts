//! Alert-rules repository — thresholds that raise notifications (and later drive webhooks/enforce).
//! Effect-wrapped Drizzle queries with typed error channels.

import { and, desc, eq } from "drizzle-orm";
import { Effect, Schema } from "effect";

import type { Db } from "../db/client";
import { alertRules } from "../db/schema";
import { NotFound, RepoError } from "../repository/errors";

// The response Schema is the single source of truth for the `AlertRule` type + the OpenAPI contract.
export const AlertRule = Schema.Struct({
  id: Schema.String,
  orgId: Schema.String,
  name: Schema.String,
  scope: Schema.String,
  metric: Schema.String,
  threshold: Schema.String,
  action: Schema.String,
  enabled: Schema.Boolean,
  accountId: Schema.NullOr(Schema.String),
  creditLimit: Schema.NullOr(Schema.String),
  windowDays: Schema.Number,
  lastStatus: Schema.NullOr(Schema.String),
  createdAt: Schema.String,
});
export type AlertRule = typeof AlertRule.Type;

export interface NewAlertRule {
  readonly orgId: string;
  readonly name: string;
  readonly scope: string;
  readonly metric: string;
  readonly threshold: string;
  readonly action: string;
  readonly enabled?: boolean | undefined;
  readonly accountId?: string | undefined;
  readonly creditLimit?: string | undefined;
  readonly windowDays?: number | undefined;
}

/** An alert rule with everything required to evaluate a budget against the engine. */
export interface EvaluableRule {
  readonly id: string;
  readonly orgId: string;
  readonly name: string;
  readonly accountId: string;
  readonly creditLimit: string;
  readonly windowDays: number;
  readonly lastStatus: string | null;
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
    accountId: row.accountId,
    creditLimit: row.creditLimit,
    windowDays: row.windowDays,
    lastStatus: row.lastStatus,
    createdAt: row.createdAt.toISOString(),
  };
}

function toEvaluable(row: typeof alertRules.$inferSelect): EvaluableRule | null {
  if (row.accountId === null || row.creditLimit === null) {
    return null;
  }
  return {
    id: row.id,
    orgId: row.orgId,
    name: row.name,
    accountId: row.accountId,
    creditLimit: row.creditLimit,
    windowDays: row.windowDays,
    lastStatus: row.lastStatus,
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
          accountId: input.accountId ?? null,
          creditLimit: input.creditLimit ?? null,
          windowDays: input.windowDays ?? 30,
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

/** Enabled budget rules for an org that have an account + credit limit to evaluate. */
export function evaluableRules(
  db: Db,
  orgId: string,
): Effect.Effect<readonly EvaluableRule[], RepoError> {
  return Effect.tryPromise({
    try: () =>
      db
        .select()
        .from(alertRules)
        .where(and(eq(alertRules.orgId, orgId), eq(alertRules.metric, "budget"))),
    catch: (cause) => new RepoError({ cause }),
  }).pipe(
    Effect.map((rows) =>
      rows
        .filter((row) => row.enabled)
        .map(toEvaluable)
        .filter((rule): rule is EvaluableRule => rule !== null),
    ),
  );
}

/** Record the most recent budget classification, so we only alert on escalation. */
export function setAlertRuleStatus(
  db: Db,
  id: string,
  status: string,
): Effect.Effect<void, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      await db.update(alertRules).set({ lastStatus: status }).where(eq(alertRules.id, id));
    },
    catch: (cause) => new RepoError({ cause }),
  });
}
