//! Alert-rule routes: create, list, and enable/disable.

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect, Schema } from "effect";

import { evaluateOrgAlertRules } from "../../alerts/evaluate";
import { createAlertRule, listAlertRules, setAlertRuleEnabled } from "../../alerts/repository";
import { Database } from "../../db/service";
import { handle } from "../errors";

const Scope = Schema.Literal("org", "team", "user", "product");
const Metric = Schema.Literal("budget", "credit", "spend");
const Action = Schema.Literal("notify", "webhook", "enforce");

const NewAlertRuleBody = Schema.Struct({
  orgId: Schema.String,
  name: Schema.String,
  scope: Scope,
  metric: Metric,
  threshold: Schema.Number,
  action: Action,
  enabled: Schema.optional(Schema.Boolean),
  accountId: Schema.optional(Schema.String),
  creditLimit: Schema.optional(Schema.Number),
  windowDays: Schema.optional(Schema.Number),
});

const EnabledBody = Schema.Struct({ enabled: Schema.Boolean });
const OrgQuery = Schema.Struct({ orgId: Schema.String });
const IdParam = Schema.Struct({ id: Schema.String });

function optionalNumberToString(value: number | undefined): string | undefined {
  if (value === undefined) {
    return undefined;
  }
  return String(value);
}

export function alertRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database> {
  return base.pipe(
    HttpRouter.get(
      "/v1/alert-rules",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const rules = yield* listAlertRules(db, orgId);
          return HttpServerResponse.unsafeJson(rules);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/alert-rules",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const body = yield* HttpServerRequest.schemaBodyJson(NewAlertRuleBody);
          const rule = yield* createAlertRule(db, {
            orgId: body.orgId,
            name: body.name,
            scope: body.scope,
            metric: body.metric,
            threshold: String(body.threshold),
            action: body.action,
            enabled: body.enabled,
            accountId: body.accountId,
            creditLimit: optionalNumberToString(body.creditLimit),
            windowDays: body.windowDays,
          });
          return HttpServerResponse.unsafeJson(rule, { status: 201 });
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/alert-rules/evaluate",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const summary = yield* evaluateOrgAlertRules(db, orgId, new Date());
          return HttpServerResponse.unsafeJson(summary);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/alert-rules/:id/enabled",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const { id } = yield* HttpRouter.schemaPathParams(IdParam);
          const { enabled } = yield* HttpServerRequest.schemaBodyJson(EnabledBody);
          const rule = yield* setAlertRuleEnabled(db, id, enabled);
          return HttpServerResponse.unsafeJson(rule);
        }),
      ),
    ),
  );
}
