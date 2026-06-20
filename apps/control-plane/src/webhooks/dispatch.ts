//! Webhook dispatch: fan a notification out to the org's matching enabled endpoints, signing each
//! delivery and recording the outcome (with retries) in the delivery log. Best-effort — dispatch
//! never fails the caller; failures land in the dead-letter view instead.

import { Duration, Effect, Match, Ref, Schedule } from "effect";

import type { Db } from "../db/client";
import type { Notification } from "../notifications/repository";
import type { WebhookSecretRow } from "./repository";
import { enabledWebhooks, recordDelivery } from "./repository";
import { signPayload } from "./signature";

const MAX_RETRIES = 2;
const TIMEOUT_MS = 5000;

/** A non-2xx response or transport error. `responseStatus` is null when no response was received. */
class DeliveryFailed {
  constructor(
    readonly responseStatus: number | null,
    readonly message: string,
  ) {}
}

function toDeliveryFailed(cause: unknown): DeliveryFailed {
  if (cause instanceof DeliveryFailed) {
    return cause;
  }
  return new DeliveryFailed(null, String(cause));
}

/** An empty event-type list means "all events". */
function matchesEvent(eventTypes: readonly string[], type: string): boolean {
  if (eventTypes.length === 0) {
    return true;
  }
  return eventTypes.includes(type);
}

function buildBody(notification: Notification): string {
  return JSON.stringify({
    event: notification.type,
    notification: {
      id: notification.id,
      orgId: notification.orgId,
      type: notification.type,
      severity: notification.severity,
      title: notification.title,
      body: notification.body,
      data: notification.data,
      createdAt: notification.createdAt,
    },
  });
}

function deliver(
  db: Db,
  webhook: WebhookSecretRow,
  notification: Notification,
): Effect.Effect<void, never> {
  const body = buildBody(notification);
  const signature = signPayload(webhook.secret, body);
  const payload: unknown = JSON.parse(body);

  return Effect.gen(function* () {
    const attempts = yield* Ref.make(0);
    const sendOnce = Effect.zipRight(
      Ref.update(attempts, (n) => n + 1),
      Effect.tryPromise({
        try: async () => {
          const response = await fetch(webhook.url, {
            method: "POST",
            headers: {
              "content-type": "application/json",
              "x-meter-event": notification.type,
              "x-meter-signature": signature,
            },
            body,
            signal: AbortSignal.timeout(TIMEOUT_MS),
          });
          if (!response.ok) {
            throw new DeliveryFailed(response.status, `endpoint responded ${response.status}`);
          }
          return response.status;
        },
        catch: toDeliveryFailed,
      }),
    );

    const schedule = Schedule.intersect(
      Schedule.recurs(MAX_RETRIES),
      Schedule.spaced(Duration.millis(50)),
    );
    const outcome = yield* sendOnce.pipe(Effect.retry(schedule), Effect.either);
    const tries = yield* Ref.get(attempts);

    const record = Match.value(outcome).pipe(
      Match.tag("Right", ({ right }) =>
        recordDelivery(db, {
          webhookId: webhook.id,
          notificationId: notification.id,
          event: notification.type,
          payload,
          status: "delivered",
          responseStatus: right,
          error: null,
          attempts: tries,
        }),
      ),
      Match.tag("Left", ({ left }) =>
        recordDelivery(db, {
          webhookId: webhook.id,
          notificationId: notification.id,
          event: notification.type,
          payload,
          status: "failed",
          responseStatus: left.responseStatus,
          error: left.message,
          attempts: tries,
        }),
      ),
      Match.exhaustive,
    );
    yield* record.pipe(Effect.ignore);
  });
}

/** Deliver a notification to every matching enabled webhook for its organization. */
export function dispatchForNotification(
  db: Db,
  notification: Notification,
): Effect.Effect<void, never> {
  return enabledWebhooks(db, notification.orgId).pipe(
    Effect.map((hooks) => hooks.filter((hook) => matchesEvent(hook.eventTypes, notification.type))),
    Effect.flatMap((hooks) =>
      Effect.forEach(hooks, (hook) => deliver(db, hook, notification), {
        concurrency: "unbounded",
        discard: true,
      }),
    ),
    Effect.catchAll(() => Effect.void),
  );
}
