//! Control-plane entrypoint: serve the CRUD API over a real Postgres connection.

import { createServer } from "node:http";

import { HttpServer } from "@effect/platform";
import { NodeHttpServer, NodeRuntime } from "@effect/platform-node";
import { Layer } from "effect";

import { makeDb } from "./db/client";
import { Database } from "./db/service";
import { router } from "./http/router";

const port = Number.parseInt(process.env.METER_CONTROL_PLANE_PORT ?? "8080", 10);
const databaseUrl =
  process.env.METER_CONTROL_PLANE_DATABASE_URL ??
  "postgres://postgres:postgres@127.0.0.1:5432/postgres";

const DatabaseLive = Layer.succeed(Database, makeDb(databaseUrl));
const ServerLive = NodeHttpServer.layer(() => createServer(), { port });

const HttpLive = HttpServer.serve(router).pipe(
  Layer.provide(DatabaseLive),
  Layer.provide(ServerLive),
);

NodeRuntime.runMain(Layer.launch(HttpLive));
