//! Control-plane entrypoint: apply config migrations, optionally start the alert-evaluation
//! scheduler, then serve the CRUD API over Postgres.

import { createServer } from "node:http";

import { HttpServer } from "@effect/platform";
import { NodeHttpServer, NodeRuntime } from "@effect/platform-node";
import { migrate } from "drizzle-orm/postgres-js/migrator";
import { Duration, Effect, Layer, Schedule } from "effect";

import { evaluateAllOrgs } from "./alerts/evaluate";
import { makeDb } from "./db/client";
import { Database } from "./db/service";
import { requireApiKey } from "./http/auth";
import { router } from "./http/router";

const port = Number.parseInt(process.env.METER_CONTROL_PLANE_PORT ?? "8090", 10);
const databaseUrl =
  process.env.METER_CONTROL_PLANE_DATABASE_URL ??
  "postgres://postgres:postgres@127.0.0.1:5432/postgres";
const evaluationIntervalSeconds = Number.parseInt(
  process.env.METER_EVALUATION_INTERVAL_SECONDS ?? "0",
  10,
);

function isTrue(value: string | undefined): boolean {
  if (value === undefined) {
    return false;
  }
  return value.toLowerCase() === "true";
}

const requireAuth = isTrue(process.env.METER_REQUIRE_AUTH);

const db = makeDb(databaseUrl);
await migrate(db, { migrationsFolder: "./drizzle" });

const HttpLive = HttpServer.serve(requireApiKey(db, requireAuth)(router)).pipe(
  Layer.provide(Layer.succeed(Database, db)),
  Layer.provide(NodeHttpServer.layer(() => createServer(), { port })),
);

// Periodically evaluate budget alert rules. Disabled (interval 0) unless configured.
const SchedulerLive = Layer.scopedDiscard(
  Effect.forkScoped(
    evaluateAllOrgs(db).pipe(
      Effect.repeat(Schedule.spaced(Duration.seconds(evaluationIntervalSeconds))),
    ),
  ),
);

function appLayer(): Layer.Layer<never, unknown> {
  if (evaluationIntervalSeconds > 0) {
    return Layer.merge(HttpLive, SchedulerLive);
  }
  return HttpLive;
}

NodeRuntime.runMain(Layer.launch(appLayer()));
