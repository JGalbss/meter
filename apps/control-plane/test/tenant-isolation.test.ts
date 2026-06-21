//! Tenant isolation (ADR 0007): an org-scoped key may act only within its own organization. Cross-org
//! reads and creates are 403; cross-org by-id mutations are 404 (the row matches nothing); organization
//! CRUD and platform-key minting are platform-only; a platform key may act across orgs.

import { HttpClient, HttpClientRequest } from "@effect/platform";
import { Effect } from "effect";
import { describe, expect, it } from "vitest";

import { createAlertRule } from "../src/alerts/repository";
import { createApiKey } from "../src/api-keys/repository";
import { createNotification } from "../src/notifications/repository";
import { createOrganization } from "../src/organizations/repository";
import { createProduct } from "../src/products/repository";
import { createWebhook } from "../src/webhooks/repository";
import { type TestDb, freshDb, runAuthed } from "./support";

const bearer = (token: string) => HttpClientRequest.setHeader("authorization", `Bearer ${token}`);

interface Fixture {
  readonly orgA: string;
  readonly orgB: string;
  readonly aAdminToken: string;
  readonly platformToken: string;
  readonly bKeyId: string;
  readonly bRuleId: string;
  readonly bWebhookId: string;
  readonly bNotificationId: string;
}

/** Seed two orgs: an org-scoped admin key for A, a platform key, and a row of each kind in B. */
async function seed(db: TestDb): Promise<Fixture> {
  const run = <A>(effect: Effect.Effect<A, unknown>): Promise<A> => Effect.runPromise(effect);
  const orgA = await run(createOrganization(db, { slug: "org-a", name: "Org A" }));
  const orgB = await run(createOrganization(db, { slug: "org-b", name: "Org B" }));
  const aAdmin = await run(
    createApiKey(db, { orgId: orgA.id, name: "a-admin", role: "admin", scope: "org" }),
  );
  const platform = await run(
    createApiKey(db, { orgId: orgA.id, name: "platform", role: "admin", scope: "platform" }),
  );
  const bKey = await run(createApiKey(db, { orgId: orgB.id, name: "b-key", role: "member" }));
  const bRule = await run(
    createAlertRule(db, {
      orgId: orgB.id,
      name: "b-rule",
      scope: "org",
      metric: "budget",
      threshold: "100",
      action: "notify",
    }),
  );
  const bWebhook = await run(
    createWebhook(db, { orgId: orgB.id, url: "https://example.test/hook", secret: "s" }),
  );
  const bNotification = await run(
    createNotification(db, { orgId: orgB.id, type: "system", severity: "info", title: "b-note" }),
  );
  return {
    orgA: orgA.id,
    orgB: orgB.id,
    aAdminToken: aAdmin.token,
    platformToken: platform.token,
    bKeyId: bKey.id,
    bRuleId: bRule.id,
    bWebhookId: bWebhook.id,
    bNotificationId: bNotification.id,
  };
}

describe("tenant isolation", () => {
  it("denies an org key reading another org's resources (403)", async () => {
    const db = await freshDb();
    const f = await seed(db);
    const result = await runAuthed(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const paths = [
          `/v1/products?orgId=${f.orgB}`,
          `/v1/alert-rules?orgId=${f.orgB}`,
          `/v1/webhooks?orgId=${f.orgB}`,
          `/v1/webhook-deliveries?orgId=${f.orgB}`,
          `/v1/notifications?orgId=${f.orgB}`,
          `/v1/api-keys?orgId=${f.orgB}`,
        ];
        const statuses: number[] = [];
        for (const path of paths) {
          const response = yield* client.execute(
            HttpClientRequest.get(path).pipe(bearer(f.aAdminToken)),
          );
          statuses.push(response.status);
        }
        return statuses;
      }),
    );
    expect(result).toEqual([403, 403, 403, 403, 403, 403]);
  });

  it("denies an org key creating resources in another org (403)", async () => {
    const db = await freshDb();
    const f = await seed(db);
    const result = await runAuthed(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const product = yield* client.execute(
          HttpClientRequest.post("/v1/products").pipe(
            bearer(f.aAdminToken),
            HttpClientRequest.bodyUnsafeJson({ orgId: f.orgB, key: "p", name: "P" }),
          ),
        );
        const webhook = yield* client.execute(
          HttpClientRequest.post("/v1/webhooks").pipe(
            bearer(f.aAdminToken),
            HttpClientRequest.bodyUnsafeJson({ orgId: f.orgB, url: "https://x.test", secret: "s" }),
          ),
        );
        return { product: product.status, webhook: webhook.status };
      }),
    );
    expect(result.product).toBe(403);
    expect(result.webhook).toBe(403);
  });

  it("denies an org key mutating another org's records by id (404)", async () => {
    const db = await freshDb();
    const f = await seed(db);
    const result = await runAuthed(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const revoke = yield* client.execute(
          HttpClientRequest.post(`/v1/api-keys/${f.bKeyId}/revoke`).pipe(bearer(f.aAdminToken)),
        );
        const toggleRule = yield* client.execute(
          HttpClientRequest.post(`/v1/alert-rules/${f.bRuleId}/enabled`).pipe(
            bearer(f.aAdminToken),
            HttpClientRequest.bodyUnsafeJson({ enabled: false }),
          ),
        );
        const toggleHook = yield* client.execute(
          HttpClientRequest.post(`/v1/webhooks/${f.bWebhookId}/enabled`).pipe(
            bearer(f.aAdminToken),
            HttpClientRequest.bodyUnsafeJson({ enabled: false }),
          ),
        );
        const readNote = yield* client.execute(
          HttpClientRequest.post(`/v1/notifications/${f.bNotificationId}/read`).pipe(
            bearer(f.aAdminToken),
          ),
        );
        return {
          revoke: revoke.status,
          toggleRule: toggleRule.status,
          toggleHook: toggleHook.status,
          readNote: readNote.status,
        };
      }),
    );
    expect(result).toEqual({ revoke: 404, toggleRule: 404, toggleHook: 404, readNote: 404 });
  });

  it("restricts organization CRUD and platform-key minting to platform keys", async () => {
    const db = await freshDb();
    const f = await seed(db);
    const result = await runAuthed(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const orgKeyList = yield* client.execute(
          HttpClientRequest.get("/v1/organizations").pipe(bearer(f.aAdminToken)),
        );
        const orgKeyCreate = yield* client.execute(
          HttpClientRequest.post("/v1/organizations").pipe(
            bearer(f.aAdminToken),
            HttpClientRequest.bodyUnsafeJson({ slug: "sneaky", name: "Sneaky" }),
          ),
        );
        const platformList = yield* client.execute(
          HttpClientRequest.get("/v1/organizations").pipe(bearer(f.platformToken)),
        );
        const escalate = yield* client.execute(
          HttpClientRequest.post("/v1/api-keys").pipe(
            bearer(f.aAdminToken),
            HttpClientRequest.bodyUnsafeJson({
              orgId: f.orgA,
              name: "escalate",
              scope: "platform",
            }),
          ),
        );
        return {
          orgKeyList: orgKeyList.status,
          orgKeyCreate: orgKeyCreate.status,
          platformList: platformList.status,
          escalate: escalate.status,
        };
      }),
    );
    expect(result.orgKeyList).toBe(403);
    expect(result.orgKeyCreate).toBe(403);
    expect(result.platformList).toBe(200);
    expect(result.escalate).toBe(403);
  });

  it("lets an org key operate within its own org, and a platform key across orgs", async () => {
    const db = await freshDb();
    const f = await seed(db);
    const result = await runAuthed(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const ownRead = yield* client.execute(
          HttpClientRequest.get(`/v1/products?orgId=${f.orgA}`).pipe(bearer(f.aAdminToken)),
        );
        const ownCreate = yield* client.execute(
          HttpClientRequest.post("/v1/products").pipe(
            bearer(f.aAdminToken),
            HttpClientRequest.bodyUnsafeJson({ orgId: f.orgA, key: "own", name: "Own" }),
          ),
        );
        const platformA = yield* client.execute(
          HttpClientRequest.get(`/v1/products?orgId=${f.orgA}`).pipe(bearer(f.platformToken)),
        );
        const platformB = yield* client.execute(
          HttpClientRequest.get(`/v1/products?orgId=${f.orgB}`).pipe(bearer(f.platformToken)),
        );
        return {
          ownRead: ownRead.status,
          ownCreate: ownCreate.status,
          platformA: platformA.status,
          platformB: platformB.status,
        };
      }),
    );
    expect(result.ownRead).toBe(200);
    expect(result.ownCreate).toBe(201);
    expect(result.platformA).toBe(200);
    expect(result.platformB).toBe(200);
  });
});
