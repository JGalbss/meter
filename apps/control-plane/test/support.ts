//! Shared test harness: an in-process HTTP test server (NodeHttpServer.layerTest) serving the
//! control-plane router over a fresh PGlite database, driven by the Effect HttpClient.

import type { HttpClient } from "@effect/platform";
import { HttpServer } from "@effect/platform";
import { NodeHttpServer } from "@effect/platform-node";
import { PGlite } from "@electric-sql/pglite";
import type { PgliteDatabase } from "drizzle-orm/pglite";
import { drizzle } from "drizzle-orm/pglite";
import { migrate } from "drizzle-orm/pglite/migrator";
import type { Scope } from "effect";
import { Effect, Layer } from "effect";

import * as schema from "../src/db/schema";
import { Database } from "../src/db/service";
import { requireApiKey } from "../src/http/auth";
import { router } from "../src/http/router";
import { CurrentPrincipalDefault } from "../src/http/tenant";

export type TestDb = PgliteDatabase<typeof schema>;

export async function freshDb(): Promise<TestDb> {
  const db = drizzle(new PGlite(), { schema });
  await migrate(db, { migrationsFolder: "./drizzle" });
  return db;
}

function testLayer(db: TestDb) {
  return HttpServer.serve(router).pipe(
    Layer.provide(Layer.succeed(Database, db)),
    Layer.provide(CurrentPrincipalDefault),
    Layer.provideMerge(NodeHttpServer.layerTest),
  );
}

/** Like `testLayer`, but with the API-key auth middleware active (for auth + tenant-isolation tests). */
function authedLayer(db: TestDb) {
  return HttpServer.serve(requireApiKey(db, true)(router)).pipe(
    Layer.provide(Layer.succeed(Database, db)),
    Layer.provide(CurrentPrincipalDefault),
    Layer.provideMerge(NodeHttpServer.layerTest),
  );
}

export function run<A, E>(
  db: TestDb,
  program: Effect.Effect<A, E, HttpClient.HttpClient | Scope.Scope>,
): Promise<A> {
  return program.pipe(Effect.scoped, Effect.provide(testLayer(db)), Effect.runPromise);
}

/** Run a program against the server with auth enforced. */
export function runAuthed<A, E>(
  db: TestDb,
  program: Effect.Effect<A, E, HttpClient.HttpClient | Scope.Scope>,
): Promise<A> {
  return program.pipe(Effect.scoped, Effect.provide(authedLayer(db)), Effect.runPromise);
}
