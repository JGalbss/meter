//! HTTP tests for notifications (raise/pull/read/ack) and alert rules (create/list/enable).

import { HttpClient, HttpClientRequest } from "@effect/platform";
import { Effect } from "effect";
import { describe, expect, it } from "vitest";

import { freshDb, run } from "./support";

function seedOrg(client: HttpClient.HttpClient) {
  return Effect.gen(function* () {
    const created = yield* client.execute(
      HttpClientRequest.post("/v1/organizations").pipe(
        HttpClientRequest.bodyUnsafeJson({ slug: "acme", name: "Acme" }),
      ),
    );
    return (yield* created.json) as { id: string };
  });
}

describe("notifications HTTP API", () => {
  it("raises, pulls, reads, and acknowledges a notification", async () => {
    const db = await freshDb();
    const result = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);

        const raised = yield* client.execute(
          HttpClientRequest.post("/v1/notifications").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              type: "budget",
              severity: "warning",
              title: "80% of cap reached",
              data: { pct: 0.8 },
            }),
          ),
        );
        expect(raised.status).toBe(201);
        const notification = (yield* raised.json) as { id: string; status: string };
        expect(notification.status).toBe("unread");

        const unread = yield* client.get(`/v1/notifications?orgId=${org.id}&status=unread`);
        const unreadList = (yield* unread.json) as ReadonlyArray<unknown>;

        const readResponse = yield* client.execute(
          HttpClientRequest.post(`/v1/notifications/${notification.id}/read`),
        );
        const afterRead = (yield* readResponse.json) as { status: string; readAt: string | null };

        const ackResponse = yield* client.execute(
          HttpClientRequest.post(`/v1/notifications/${notification.id}/ack`),
        );
        const afterAck = (yield* ackResponse.json) as { status: string; ackedAt: string | null };

        const acked = yield* client.get(`/v1/notifications?orgId=${org.id}&status=acked`);
        const ackedList = (yield* acked.json) as ReadonlyArray<unknown>;

        return { unreadList, afterRead, afterAck, ackedList };
      }),
    );

    expect(result.unreadList).toHaveLength(1);
    expect(result.afterRead.status).toBe("read");
    expect(result.afterRead.readAt).not.toBeNull();
    expect(result.afterAck.status).toBe("acked");
    expect(result.afterAck.ackedAt).not.toBeNull();
    expect(result.ackedList).toHaveLength(1);
  });

  it("404s when reading an unknown notification", async () => {
    const db = await freshDb();
    const status = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const response = yield* client.execute(
          HttpClientRequest.post("/v1/notifications/00000000-0000-0000-0000-000000000000/read"),
        );
        return response.status;
      }),
    );
    expect(status).toBe(404);
  });

  it("rejects an invalid severity with 400", async () => {
    const db = await freshDb();
    const status = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);
        const response = yield* client.execute(
          HttpClientRequest.post("/v1/notifications").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              type: "budget",
              severity: "catastrophic",
              title: "nope",
            }),
          ),
        );
        return response.status;
      }),
    );
    expect(status).toBe(400);
  });
});

describe("alert-rules HTTP API", () => {
  it("creates, lists, and toggles an alert rule", async () => {
    const db = await freshDb();
    const result = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);

        const created = yield* client.execute(
          HttpClientRequest.post("/v1/alert-rules").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              name: "Budget 80%",
              scope: "org",
              metric: "budget",
              threshold: 0.8,
              action: "notify",
            }),
          ),
        );
        expect(created.status).toBe(201);
        const rule = (yield* created.json) as { id: string; enabled: boolean; threshold: string };

        const listed = yield* client.get(`/v1/alert-rules?orgId=${org.id}`);
        const rules = (yield* listed.json) as ReadonlyArray<unknown>;

        const toggled = yield* client.execute(
          HttpClientRequest.post(`/v1/alert-rules/${rule.id}/enabled`).pipe(
            HttpClientRequest.bodyUnsafeJson({ enabled: false }),
          ),
        );
        const afterToggle = (yield* toggled.json) as { enabled: boolean };

        return { rule, rules, afterToggle };
      }),
    );

    expect(result.rule.enabled).toBe(true);
    expect(result.rule.threshold).toBe("0.8");
    expect(result.rules).toHaveLength(1);
    expect(result.afterToggle.enabled).toBe(false);
  });

  it("404s when toggling an unknown rule", async () => {
    const db = await freshDb();
    const status = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const response = yield* client.execute(
          HttpClientRequest.post(
            "/v1/alert-rules/00000000-0000-0000-0000-000000000000/enabled",
          ).pipe(HttpClientRequest.bodyUnsafeJson({ enabled: false })),
        );
        return response.status;
      }),
    );
    expect(status).toBe(404);
  });
});
