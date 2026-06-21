//! API-key auth: CRUD (mint/list/revoke, token shown once) over the normal harness, and enforcement
//! via the auth middleware (401 without a key, 200 with one, `/health` always open).

import { HttpClient, HttpClientRequest, HttpServer } from "@effect/platform";
import { NodeHttpServer } from "@effect/platform-node";
import type { Scope } from "effect";
import { Effect, Layer } from "effect";
import { describe, expect, it } from "vitest";

import { createApiKey } from "../src/api-keys/repository";
import { Database } from "../src/db/service";
import { requireApiKey } from "../src/http/auth";
import { router } from "../src/http/router";
import { CurrentPrincipalDefault } from "../src/http/tenant";
import { createOrganization } from "../src/organizations/repository";
import { type TestDb, freshDb, run } from "./support";

function authedLayer(db: TestDb) {
  return HttpServer.serve(requireApiKey(db, true)(router)).pipe(
    Layer.provide(Layer.succeed(Database, db)),
    Layer.provide(CurrentPrincipalDefault),
    Layer.provideMerge(NodeHttpServer.layerTest),
  );
}

function runAuthed<A, E>(
  db: TestDb,
  program: Effect.Effect<A, E, HttpClient.HttpClient | Scope.Scope>,
): Promise<A> {
  return program.pipe(Effect.scoped, Effect.provide(authedLayer(db)), Effect.runPromise);
}

describe("API-key CRUD", () => {
  it("mints (token once), lists without the token, and revokes", async () => {
    const db = await freshDb();
    const result = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const orgResponse = yield* client.execute(
          HttpClientRequest.post("/v1/organizations").pipe(
            HttpClientRequest.bodyUnsafeJson({ slug: "acme", name: "Acme" }),
          ),
        );
        const org = (yield* orgResponse.json) as { id: string };

        const created = yield* client.execute(
          HttpClientRequest.post("/v1/api-keys").pipe(
            HttpClientRequest.bodyUnsafeJson({ orgId: org.id, name: "ci" }),
          ),
        );
        expect(created.status).toBe(201);
        const key = (yield* created.json) as { id: string; token: string; prefix: string };

        const listed = yield* client.get(`/v1/api-keys?orgId=${org.id}`);
        const keys = (yield* listed.json) as ReadonlyArray<Record<string, unknown>>;

        const revoked = yield* client.execute(
          HttpClientRequest.post(`/v1/api-keys/${key.id}/revoke`),
        );
        const revokedKey = (yield* revoked.json) as { revokedAt: string | null };

        return { key, keys, revokedKey };
      }),
    );

    expect(result.key.token.startsWith("mk_")).toBe(true);
    expect(result.keys).toHaveLength(1);
    expect(result.keys[0]?.token).toBeUndefined();
    expect(result.keys[0]?.tokenHash).toBeUndefined();
    expect(result.revokedKey.revokedAt).not.toBeNull();
  });
});

describe("API-key enforcement", () => {
  it("allows /health, rejects unauthenticated /v1, and accepts a valid key", async () => {
    const db = await freshDb();
    const org = await Effect.runPromise(createOrganization(db, { slug: "acme", name: "Acme" }));
    // A platform key: /v1/organizations is platform-scoped, and this test asserts a valid key → 200.
    const key = await Effect.runPromise(
      createApiKey(db, { orgId: org.id, name: "ci", scope: "platform" }),
    );

    const result = await runAuthed(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;

        const health = yield* client.get("/health");
        const noKey = yield* client.get("/v1/organizations");
        const withKey = yield* client.execute(
          HttpClientRequest.get("/v1/organizations").pipe(
            HttpClientRequest.setHeader("authorization", `Bearer ${key.token}`),
          ),
        );
        const badKey = yield* client.execute(
          HttpClientRequest.get("/v1/organizations").pipe(
            HttpClientRequest.setHeader("authorization", "Bearer mk_not_a_real_key"),
          ),
        );

        return {
          health: health.status,
          noKey: noKey.status,
          withKey: withKey.status,
          badKey: badKey.status,
        };
      }),
    );

    expect(result.health).toBe(200);
    expect(result.noKey).toBe(401);
    expect(result.withKey).toBe(200);
    expect(result.badKey).toBe(401);
  });
});

describe("RBAC", () => {
  it("enforces the role hierarchy per method and resource", async () => {
    const db = await freshDb();
    const org = await Effect.runPromise(createOrganization(db, { slug: "rbac", name: "RBAC" }));
    const viewer = await Effect.runPromise(
      createApiKey(db, { orgId: org.id, name: "viewer", role: "viewer" }),
    );
    const member = await Effect.runPromise(
      createApiKey(db, { orgId: org.id, name: "member", role: "member" }),
    );
    const admin = await Effect.runPromise(
      createApiKey(db, { orgId: org.id, name: "admin", role: "admin" }),
    );

    const bearer = (token: string) =>
      HttpClientRequest.setHeader("authorization", `Bearer ${token}`);

    const result = await runAuthed(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;

        // viewer: reads allowed, any write denied.
        const viewerRead = yield* client.execute(
          HttpClientRequest.get(`/v1/products?orgId=${org.id}`).pipe(bearer(viewer.token)),
        );
        const viewerWrite = yield* client.execute(
          HttpClientRequest.post("/v1/products").pipe(
            bearer(viewer.token),
            HttpClientRequest.bodyUnsafeJson({ orgId: org.id, key: "p1", name: "P1" }),
          ),
        );

        // member: ordinary writes allowed, credential management denied.
        const memberWrite = yield* client.execute(
          HttpClientRequest.post("/v1/products").pipe(
            bearer(member.token),
            HttpClientRequest.bodyUnsafeJson({ orgId: org.id, key: "p2", name: "P2" }),
          ),
        );
        const memberKeyMgmt = yield* client.execute(
          HttpClientRequest.post("/v1/api-keys").pipe(
            bearer(member.token),
            HttpClientRequest.bodyUnsafeJson({ orgId: org.id, name: "nope" }),
          ),
        );

        // admin: credential management allowed.
        const adminKeyMgmt = yield* client.execute(
          HttpClientRequest.post("/v1/api-keys").pipe(
            bearer(admin.token),
            HttpClientRequest.bodyUnsafeJson({ orgId: org.id, name: "ok", role: "member" }),
          ),
        );

        return {
          viewerRead: viewerRead.status,
          viewerWrite: viewerWrite.status,
          memberWrite: memberWrite.status,
          memberKeyMgmt: memberKeyMgmt.status,
          adminKeyMgmt: adminKeyMgmt.status,
        };
      }),
    );

    expect(result.viewerRead).toBe(200);
    expect(result.viewerWrite).toBe(403);
    expect(result.memberWrite).toBe(201);
    expect(result.memberKeyMgmt).toBe(403);
    expect(result.adminKeyMgmt).toBe(201);
  });

  it("defaults keys minted without a role to admin (backward compatible)", async () => {
    const db = await freshDb();
    const org = await Effect.runPromise(createOrganization(db, { slug: "legacy", name: "Legacy" }));
    const key = await Effect.runPromise(createApiKey(db, { orgId: org.id, name: "legacy" }));
    expect(key.role).toBe("admin");
  });
});
