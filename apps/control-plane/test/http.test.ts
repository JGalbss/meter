//! End-to-end HTTP tests for the control-plane API: drive the router over the in-process test server
//! (NodeHttpServer.layerTest) backed by PGlite, exercising it with the Effect HttpClient.

import { HttpClient, HttpClientRequest, HttpServer } from "@effect/platform";
import { NodeHttpServer } from "@effect/platform-node";
import { PGlite } from "@electric-sql/pglite";
import type { PgliteDatabase } from "drizzle-orm/pglite";
import { drizzle } from "drizzle-orm/pglite";
import { migrate } from "drizzle-orm/pglite/migrator";
import { Effect, Layer } from "effect";
import type { Scope } from "effect";
import { describe, expect, it } from "vitest";

import * as schema from "../src/db/schema";
import { Database } from "../src/db/service";
import { router } from "../src/http/router";

async function freshDb(): Promise<PgliteDatabase<typeof schema>> {
  const db = drizzle(new PGlite(), { schema });
  await migrate(db, { migrationsFolder: "./drizzle" });
  return db;
}

/** A live test server (ephemeral port) serving `router` over the given database, exposing HttpClient. */
function testLayer(db: PgliteDatabase<typeof schema>) {
  return HttpServer.serve(router).pipe(
    Layer.provide(Layer.succeed(Database, db)),
    Layer.provideMerge(NodeHttpServer.layerTest),
  );
}

function run<A, E>(
  db: PgliteDatabase<typeof schema>,
  program: Effect.Effect<A, E, HttpClient.HttpClient | Scope.Scope>,
): Promise<A> {
  return program.pipe(Effect.scoped, Effect.provide(testLayer(db)), Effect.runPromise);
}

describe("control-plane HTTP API", () => {
  it("reports health", async () => {
    const db = await freshDb();
    const body = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const response = yield* client.get("/health");
        expect(response.status).toBe(200);
        return yield* response.json;
      }),
    );
    expect(body).toEqual({ status: "ok" });
  });

  it("creates and lists organizations and their products", async () => {
    const db = await freshDb();
    const result = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;

        const created = yield* client.execute(
          HttpClientRequest.post("/v1/organizations").pipe(
            HttpClientRequest.bodyUnsafeJson({ slug: "acme", name: "Acme" }),
          ),
        );
        expect(created.status).toBe(201);
        const org = (yield* created.json) as { id: string; slug: string };

        const orgsResponse = yield* client.get("/v1/organizations");
        const orgs = (yield* orgsResponse.json) as ReadonlyArray<{ slug: string }>;

        const productResponse = yield* client.execute(
          HttpClientRequest.post("/v1/products").pipe(
            HttpClientRequest.bodyUnsafeJson({ orgId: org.id, key: "chat", name: "Chat" }),
          ),
        );
        expect(productResponse.status).toBe(201);

        const listResponse = yield* client.get(`/v1/products?orgId=${org.id}`);
        const products = (yield* listResponse.json) as ReadonlyArray<{
          orgId: string;
          key: string;
          name: string;
        }>;

        return { org, orgs, products };
      }),
    );

    expect(result.org.slug).toBe("acme");
    expect(result.orgs).toHaveLength(1);
    expect(result.products).toHaveLength(1);
    expect(result.products[0]).toMatchObject({ orgId: result.org.id, key: "chat", name: "Chat" });
  });

  it("rejects an invalid body with 400", async () => {
    const db = await freshDb();
    const status = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const response = yield* client.execute(
          HttpClientRequest.post("/v1/organizations").pipe(
            HttpClientRequest.bodyUnsafeJson({ slug: "missing-name" }),
          ),
        );
        return response.status;
      }),
    );
    expect(status).toBe(400);
  });

  it("rejects a missing query parameter with 400", async () => {
    const db = await freshDb();
    const status = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const response = yield* client.get("/v1/products");
        return response.status;
      }),
    );
    expect(status).toBe(400);
  });
});
