//! Alert-rule routes: create, list, and enable/disable.

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Clock, Effect, Schema } from "effect";

import { evaluateOrgAlertRules } from "../../alerts/evaluate";
import { createAlertRule, listAlertRules, setAlertRuleEnabled } from "../../alerts/repository";
import { Database } from "../../db/service";
import { forbidden, handle } from "../errors";
import { CurrentPrincipal, authorizeOrg, isAllowed, orgScope } from "../tenant";

const Scope = Schema.Literal("org", "team", "user", "product");
const Metric = Schema.Literal("budget", "credit", "spend");
const Action = Schema.Literal("notify", "webhook", "enforce");

export const NewAlertRuleBody = Schema.Struct({
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
): HttpRouter.HttpRouter<E, R | Database | CurrentPrincipal> {
  return base.pipe(
    HttpRouter.get(
      "/v1/alert-rules",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const access = authorizeOrg(principal, orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const rules = yield* listAlertRules(db, access.orgId);
          return HttpServerResponse.unsafeJson(rules);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/alert-rules",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const body = yield* HttpServerRequest.schemaBodyJson(NewAlertRuleBody);
          const access = authorizeOrg(principal, body.orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const rule = yield* createAlertRule(db, {
            orgId: access.orgId,
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
          const principal = yield* CurrentPrincipal;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const access = authorizeOrg(principal, orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const now = new Date(yield* Clock.currentTimeMillis);
          const summary = yield* evaluateOrgAlertRules(db, access.orgId, now);
          return HttpServerResponse.unsafeJson(summary);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/alert-rules/:id/enabled",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { id } = yield* HttpRouter.schemaPathParams(IdParam);
          const { enabled } = yield* HttpServerRequest.schemaBodyJson(EnabledBody);
          const rule = yield* setAlertRuleEnabled(db, id, orgScope(principal), enabled);
          return HttpServerResponse.unsafeJson(rule);
        }),
      ),
    ),
  );
}
