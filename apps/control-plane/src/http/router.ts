//! HTTP router: the control-plane CRUD surface the dashboard hits. Config and operational state only —
//! money lives in the engine. Routes are composed from one module per resource. No money is computed
//! here.

import { HttpRouter, HttpServerResponse } from "@effect/platform";
import { sql } from "drizzle-orm";
import { Effect, Either } from "effect";

import { Database } from "../db/service";
import { openApiDocument } from "./openapi";
import { alertRoutes } from "./routes/alerts";
import { apiKeyRoutes } from "./routes/api-keys";
import { notificationRoutes } from "./routes/notifications";
import { agentRoutes } from "./routes/agents";
import { organizationRoutes } from "./routes/organizations";
import { webhookRoutes } from "./routes/webhooks";
import type { CurrentPrincipal } from "./tenant";

export const router: HttpRouter.HttpRouter<never, Database | CurrentPrincipal> =
  HttpRouter.empty.pipe(
    HttpRouter.get("/health", Effect.succeed(HttpServerResponse.unsafeJson({ status: "ok" }))),
    HttpRouter.get(
      "/health/ready",
      Effect.gen(function* () {
        // Readiness gates traffic on the config database being reachable (liveness stays static so a
        // transient blip never trips a restart). A `503` tells the load balancer to hold off.
        const db = yield* Database;
        const probe = yield* Effect.either(
          Effect.tryPromise(async () => {
            await db.execute(sql`select 1`);
          }),
        );
        if (Either.isLeft(probe)) {
          return HttpServerResponse.unsafeJson(
            { status: "unavailable", database: false },
            { status: 503 },
          );
        }
        return HttpServerResponse.unsafeJson({ status: "ok", database: true });
      }),
    ),
    HttpRouter.get("/openapi.json", Effect.succeed(HttpServerResponse.unsafeJson(openApiDocument))),
    organizationRoutes,
    agentRoutes,
    notificationRoutes,
    alertRoutes,
    webhookRoutes,
    apiKeyRoutes,
  );
