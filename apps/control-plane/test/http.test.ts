//! End-to-end HTTP tests for the control-plane API: drive the router over the in-process test server
//! (NodeHttpServer.layerTest) backed by PGlite, exercising it with the Effect HttpClient.

import { HttpClient, HttpClientRequest } from "@effect/platform";
import { Effect } from "effect";
import { describe, expect, it } from "vitest";

import { freshDb, run } from "./support";

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

  it("reports readiness when the database is reachable", async () => {
    const db = await freshDb();
    const result = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const response = yield* client.get("/health/ready");
        return { status: response.status, body: yield* response.json };
      }),
    );
    expect(result.status).toBe(200);
    expect(result.body).toEqual({ status: "ok", database: true });
  });

  it("creates and lists organizations and their agents", async () => {
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

        const agentResponse = yield* client.execute(
          HttpClientRequest.post("/v1/agents").pipe(
            HttpClientRequest.bodyUnsafeJson({ orgId: org.id, key: "chat", name: "Chat" }),
          ),
        );
        expect(agentResponse.status).toBe(201);

        const listResponse = yield* client.get(`/v1/agents?orgId=${org.id}`);
        const agents = (yield* listResponse.json) as ReadonlyArray<{
          orgId: string;
          key: string;
          name: string;
        }>;

        return { org, orgs, agents };
      }),
    );

    expect(result.org.slug).toBe("acme");
    expect(result.orgs).toHaveLength(1);
    expect(result.agents).toHaveLength(1);
    expect(result.agents[0]).toMatchObject({ orgId: result.org.id, key: "chat", name: "Chat" });
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
        const response = yield* client.get("/v1/agents");
        return response.status;
      }),
    );
    expect(status).toBe(400);
  });
});
