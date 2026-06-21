//! Notification routes: raise, list (pull), mark read, and acknowledge.

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Clock, Effect, Schema } from "effect";

import { Database } from "../../db/service";
import {
  ackNotification,
  createNotification,
  listNotifications,
  markNotificationRead,
} from "../../notifications/repository";
import { dispatchForNotification } from "../../webhooks/dispatch";
import { forbidden, handle } from "../errors";
import { CurrentPrincipal, authorizeOrg, isAllowed, orgScope } from "../tenant";

const Severity = Schema.Literal("info", "warning", "critical");
const NotificationType = Schema.Literal("budget", "credit", "invoice", "run_failure", "system");

export const NewNotificationBody = Schema.Struct({
  orgId: Schema.String,
  type: NotificationType,
  severity: Severity,
  title: Schema.String,
  body: Schema.optional(Schema.String),
  data: Schema.optional(Schema.Unknown),
});

const ListQuery = Schema.Struct({
  orgId: Schema.String,
  status: Schema.optional(Schema.String),
});

const IdParam = Schema.Struct({ id: Schema.String });

export function notificationRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database | CurrentPrincipal> {
  return base.pipe(
    HttpRouter.get(
      "/v1/notifications",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { orgId, status } = yield* HttpServerRequest.schemaSearchParams(ListQuery);
          const access = authorizeOrg(principal, orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const items = yield* listNotifications(db, access.orgId, status);
          return HttpServerResponse.unsafeJson(items);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/notifications",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const body = yield* HttpServerRequest.schemaBodyJson(NewNotificationBody);
          const access = authorizeOrg(principal, body.orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const notification = yield* createNotification(db, { ...body, orgId: access.orgId });
          yield* dispatchForNotification(db, notification);
          return HttpServerResponse.unsafeJson(notification, { status: 201 });
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/notifications/:id/read",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { id } = yield* HttpRouter.schemaPathParams(IdParam);
          const now = new Date(yield* Clock.currentTimeMillis);
          const updated = yield* markNotificationRead(db, id, orgScope(principal), now);
          return HttpServerResponse.unsafeJson(updated);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/notifications/:id/ack",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { id } = yield* HttpRouter.schemaPathParams(IdParam);
          const now = new Date(yield* Clock.currentTimeMillis);
          const updated = yield* ackNotification(db, id, orgScope(principal), now);
          return HttpServerResponse.unsafeJson(updated);
        }),
      ),
    ),
  );
}
