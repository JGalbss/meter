//! Webhook routes: register endpoints, list them, enable/disable, and read the delivery log.

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect, Schema } from "effect";

import { Database } from "../../db/service";
import {
  createWebhook,
  listDeliveries,
  listWebhooks,
  setWebhookEnabled,
} from "../../webhooks/repository";
import { forbidden, handle } from "../errors";
import { CurrentPrincipal, authorizeOrg, isAllowed, orgScope } from "../tenant";

export const NewWebhookBody = Schema.Struct({
  orgId: Schema.String,
  url: Schema.String,
  secret: Schema.String,
  eventTypes: Schema.optional(Schema.Array(Schema.String)),
});

export const EnabledBody = Schema.Struct({ enabled: Schema.Boolean });
const OrgQuery = Schema.Struct({ orgId: Schema.String });
const IdParam = Schema.Struct({ id: Schema.String });

export function webhookRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database | CurrentPrincipal> {
  return base.pipe(
    HttpRouter.get(
      "/v1/webhooks",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const access = authorizeOrg(principal, orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const hooks = yield* listWebhooks(db, access.orgId);
          return HttpServerResponse.unsafeJson(hooks);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/webhooks",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const body = yield* HttpServerRequest.schemaBodyJson(NewWebhookBody);
          const access = authorizeOrg(principal, body.orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const webhook = yield* createWebhook(db, { ...body, orgId: access.orgId });
          return HttpServerResponse.unsafeJson(webhook, { status: 201 });
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/webhooks/:id/enabled",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { id } = yield* HttpRouter.schemaPathParams(IdParam);
          const { enabled } = yield* HttpServerRequest.schemaBodyJson(EnabledBody);
          const webhook = yield* setWebhookEnabled(db, id, orgScope(principal), enabled);
          return HttpServerResponse.unsafeJson(webhook);
        }),
      ),
    ),
    HttpRouter.get(
      "/v1/webhook-deliveries",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const access = authorizeOrg(principal, orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const deliveries = yield* listDeliveries(db, access.orgId);
          return HttpServerResponse.unsafeJson(deliveries);
        }),
      ),
    ),
  );
}
