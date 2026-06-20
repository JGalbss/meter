//! Control-plane entrypoint: apply config migrations, then serve the CRUD API over Postgres.

import { createServer } from "node:http";

import { HttpServer } from "@effect/platform";
import { NodeHttpServer, NodeRuntime } from "@effect/platform-node";
import { migrate } from "drizzle-orm/postgres-js/migrator";
import { Layer } from "effect";

import { makeDb } from "./db/client";
import { Database } from "./db/service";
import { router } from "./http/router";

const port = Number.parseInt(process.env.METER_CONTROL_PLANE_PORT ?? "8090", 10);
const databaseUrl =
  process.env.METER_CONTROL_PLANE_DATABASE_URL ??
  "postgres://postgres:postgres@127.0.0.1:5432/postgres";

const db = makeDb(databaseUrl);
await migrate(db, { migrationsFolder: "./drizzle" });

const HttpLive = HttpServer.serve(router).pipe(
  Layer.provide(Layer.succeed(Database, db)),
  Layer.provide(NodeHttpServer.layer(() => createServer(), { port })),
);

NodeRuntime.runMain(Layer.launch(HttpLive));
