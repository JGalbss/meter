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
import { handle } from "../errors";

const NewWebhookBody = Schema.Struct({
  orgId: Schema.String,
  url: Schema.String,
  secret: Schema.String,
  eventTypes: Schema.optional(Schema.Array(Schema.String)),
});

const EnabledBody = Schema.Struct({ enabled: Schema.Boolean });
const OrgQuery = Schema.Struct({ orgId: Schema.String });
const IdParam = Schema.Struct({ id: Schema.String });

export function webhookRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database> {
  return base.pipe(
    HttpRouter.get(
      "/v1/webhooks",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const hooks = yield* listWebhooks(db, orgId);
          return HttpServerResponse.unsafeJson(hooks);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/webhooks",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const body = yield* HttpServerRequest.schemaBodyJson(NewWebhookBody);
          const webhook = yield* createWebhook(db, body);
          return HttpServerResponse.unsafeJson(webhook, { status: 201 });
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/webhooks/:id/enabled",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const { id } = yield* HttpRouter.schemaPathParams(IdParam);
          const { enabled } = yield* HttpServerRequest.schemaBodyJson(EnabledBody);
          const webhook = yield* setWebhookEnabled(db, id, enabled);
          return HttpServerResponse.unsafeJson(webhook);
        }),
      ),
    ),
    HttpRouter.get(
      "/v1/webhook-deliveries",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const deliveries = yield* listDeliveries(db, orgId);
          return HttpServerResponse.unsafeJson(deliveries);
        }),
      ),
    ),
  );
}
