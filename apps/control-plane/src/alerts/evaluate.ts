//! Alert evaluation: for each budget rule, ask the engine to classify the account's usage and raise a
//! notification (which dispatches webhooks) when the status escalates. State transitions, not levels,
//! drive alerts — so a sustained `exceeded` does not spam, but `ok -> warning -> exceeded` each fire.

import { Effect } from "effect";

import type { Db } from "../db/client";
import { type BudgetStatus, fetchBudgetStatus } from "../engine/client";
import { createNotification } from "../notifications/repository";
import { listOrganizations } from "../organizations/repository";
import { dispatchForNotification } from "../webhooks/dispatch";
import { type EvaluableRule, evaluableRules, setAlertRuleStatus } from "./repository";

const SEVERITY_RANK: Record<string, number> = { ok: 0, warning: 1, exceeded: 2 };
const NOTIFICATION_SEVERITY: Record<string, string> = { warning: "warning", exceeded: "critical" };

const DAY_MS = 86_400_000;

export interface EvaluationSummary {
  readonly evaluated: number;
  readonly raised: number;
}

export interface AllOrgsSummary {
  readonly orgs: number;
  readonly evaluated: number;
  readonly raised: number;
}

function isEscalation(current: string, previous: string): boolean {
  return (SEVERITY_RANK[current] ?? 0) > (SEVERITY_RANK[previous] ?? 0);
}

function raise(db: Db, rule: EvaluableRule, budget: BudgetStatus): Effect.Effect<unknown, unknown> {
  return createNotification(db, {
    orgId: rule.orgId,
    type: "budget",
    severity: NOTIFICATION_SEVERITY[budget.status] ?? "warning",
    title: `${rule.name}: budget ${budget.status}`,
    body: `Account ${rule.accountId} used ${budget.used_credits} of ${budget.limit_credits} credits (${budget.ratio}).`,
    data: {
      accountId: rule.accountId,
      status: budget.status,
      ratio: budget.ratio,
      used: budget.used_credits,
      limit: budget.limit_credits,
    },
  }).pipe(Effect.tap((notification) => dispatchForNotification(db, notification)));
}

function processRule(db: Db, rule: EvaluableRule, now: Date): Effect.Effect<number, never> {
  const end = now.toISOString();
  const start = new Date(now.getTime() - rule.windowDays * DAY_MS).toISOString();
  return fetchBudgetStatus({ accountId: rule.accountId, limit: rule.creditLimit, start, end }).pipe(
    Effect.flatMap((budget) => {
      const escalated = isEscalation(budget.status, rule.lastStatus ?? "ok");
      const persist = setAlertRuleStatus(db, rule.id, budget.status);
      if (!escalated) {
        return persist.pipe(Effect.as(0));
      }
      return raise(db, rule, budget).pipe(Effect.zipRight(persist), Effect.as(1));
    }),
    Effect.catchAll(() => Effect.succeed(0)),
  );
}

/** Evaluate every budget alert rule for an organization. Best-effort: a failing rule yields 0 raised. */
export function evaluateOrgAlertRules(
  db: Db,
  orgId: string,
  now: Date,
): Effect.Effect<EvaluationSummary, never> {
  return evaluableRules(db, orgId).pipe(
    Effect.flatMap((rules) =>
      Effect.forEach(rules, (rule) => processRule(db, rule, now)).pipe(
        Effect.map((raisedPerRule) => ({
          evaluated: rules.length,
          raised: raisedPerRule.reduce((total, raised) => total + raised, 0),
        })),
      ),
    ),
    Effect.catchAll(() => Effect.succeed({ evaluated: 0, raised: 0 })),
  );
}

/** Evaluate budget rules across every organization (the scheduler's unit of work). Best-effort.
 * Wrapped in `suspend` so each scheduled run samples a fresh timestamp. */
export function evaluateAllOrgs(db: Db): Effect.Effect<AllOrgsSummary, never> {
  return Effect.suspend(() => {
    const now = new Date();
    return listOrganizations(db).pipe(
      Effect.flatMap((orgs) =>
        Effect.forEach(orgs, (org) => evaluateOrgAlertRules(db, org.id, now)).pipe(
          Effect.map((summaries) => ({
            orgs: orgs.length,
            evaluated: summaries.reduce((total, summary) => total + summary.evaluated, 0),
            raised: summaries.reduce((total, summary) => total + summary.raised, 0),
          })),
        ),
      ),
      Effect.catchAll(() => Effect.succeed({ orgs: 0, evaluated: 0, raised: 0 })),
    );
  });
}
