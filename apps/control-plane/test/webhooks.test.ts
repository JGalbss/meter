//! Tests for webhooks: signature, CRUD, and real end-to-end delivery against a local HTTP sink
//! (success, event-type filtering, and failure → dead-letter).

import { createServer } from "node:http";

import { HttpClient, HttpClientRequest } from "@effect/platform";
import { Effect } from "effect";
import { afterEach, describe, expect, it } from "vitest";

import { isValidSignature, signPayload } from "../src/webhooks/signature";
import { freshDb, run } from "./support";

interface Captured {
  readonly headers: Record<string, string | string[] | undefined>;
  readonly body: string;
}

interface Sink {
  readonly url: string;
  readonly received: Captured[];
  readonly close: () => Promise<void>;
  readonly port: number;
}

async function startSink(status: number): Promise<Sink> {
  const received: Captured[] = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      received.push({ headers: req.headers, body });
      res.writeHead(status);
      res.end();
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error("sink has no port");
  }
  const port = address.port;
  return {
    url: `http://127.0.0.1:${port}`,
    received,
    port,
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  };
}

const sinks: Sink[] = [];
async function sink(status = 200): Promise<Sink> {
  const created = await startSink(status);
  sinks.push(created);
  return created;
}

afterEach(async () => {
  await Promise.all(sinks.splice(0).map((s) => s.close()));
});

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

describe("webhook signature", () => {
  it("verifies a matching signature and rejects a tampered one", () => {
    const body = JSON.stringify({ event: "budget" });
    const signature = signPayload("shh", body);
    expect(isValidSignature("shh", body, signature)).toBe(true);
    expect(isValidSignature("shh", `${body} `, signature)).toBe(false);
    expect(isValidSignature("other", body, signature)).toBe(false);
  });
});

describe("webhooks HTTP API", () => {
  it("registers a webhook without leaking the secret, lists it, and disables it", async () => {
    const db = await freshDb();
    const result = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);

        const created = yield* client.execute(
          HttpClientRequest.post("/v1/webhooks").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              url: "https://example.test/hook",
              secret: "shh",
              eventTypes: ["budget"],
            }),
          ),
        );
        expect(created.status).toBe(201);
        const webhook = (yield* created.json) as { id: string; secret?: string; enabled: boolean };

        const listed = yield* client.get(`/v1/webhooks?orgId=${org.id}`);
        const hooks = (yield* listed.json) as ReadonlyArray<unknown>;

        const toggled = yield* client.execute(
          HttpClientRequest.post(`/v1/webhooks/${webhook.id}/enabled`).pipe(
            HttpClientRequest.bodyUnsafeJson({ enabled: false }),
          ),
        );
        const afterToggle = (yield* toggled.json) as { enabled: boolean };

        return { webhook, hooks, afterToggle };
      }),
    );

    expect(result.webhook.secret).toBeUndefined();
    expect(result.webhook.enabled).toBe(true);
    expect(result.hooks).toHaveLength(1);
    expect(result.afterToggle.enabled).toBe(false);
  });
});

describe("webhook delivery", () => {
  it("delivers a signed payload to a matching endpoint and logs it", async () => {
    const endpoint = await sink(200);
    const db = await freshDb();
    const deliveries = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);

        yield* client.execute(
          HttpClientRequest.post("/v1/webhooks").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              url: endpoint.url,
              secret: "shh",
              eventTypes: ["budget"],
            }),
          ),
        );

        yield* client.execute(
          HttpClientRequest.post("/v1/notifications").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              type: "budget",
              severity: "warning",
              title: "80% of cap reached",
            }),
          ),
        );

        const log = yield* client.get(`/v1/webhook-deliveries?orgId=${org.id}`);
        return (yield* log.json) as ReadonlyArray<{
          status: string;
          responseStatus: number | null;
          attempts: number;
        }>;
      }),
    );

    expect(endpoint.received).toHaveLength(1);
    const request = endpoint.received[0];
    if (request === undefined) {
      throw new Error("sink received nothing");
    }
    expect(request.headers["x-meter-event"]).toBe("budget");
    const signature = request.headers["x-meter-signature"];
    expect(typeof signature).toBe("string");
    expect(isValidSignature("shh", request.body, signature as string)).toBe(true);

    expect(deliveries).toHaveLength(1);
    expect(deliveries[0]?.status).toBe("delivered");
    expect(deliveries[0]?.responseStatus).toBe(200);
    expect(deliveries[0]?.attempts).toBe(1);
  });

  it("does not deliver to an endpoint subscribed to other event types", async () => {
    const endpoint = await sink(200);
    const db = await freshDb();
    const deliveries = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);

        yield* client.execute(
          HttpClientRequest.post("/v1/webhooks").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              url: endpoint.url,
              secret: "shh",
              eventTypes: ["credit"],
            }),
          ),
        );

        yield* client.execute(
          HttpClientRequest.post("/v1/notifications").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              type: "budget",
              severity: "info",
              title: "unrelated",
            }),
          ),
        );

        const log = yield* client.get(`/v1/webhook-deliveries?orgId=${org.id}`);
        return (yield* log.json) as ReadonlyArray<unknown>;
      }),
    );

    expect(endpoint.received).toHaveLength(0);
    expect(deliveries).toHaveLength(0);
  });

  it("records a failed delivery (dead-letter) after exhausting retries", async () => {
    // Bind then immediately release a port so it is guaranteed closed (connection refused).
    const closed = await startSink(200);
    const deadUrl = closed.url;
    await closed.close();

    const db = await freshDb();
    const deliveries = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);

        yield* client.execute(
          HttpClientRequest.post("/v1/webhooks").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              url: deadUrl,
              secret: "shh",
            }),
          ),
        );

        yield* client.execute(
          HttpClientRequest.post("/v1/notifications").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              type: "system",
              severity: "critical",
              title: "will fail",
            }),
          ),
        );

        const log = yield* client.get(`/v1/webhook-deliveries?orgId=${org.id}`);
        return (yield* log.json) as ReadonlyArray<{ status: string; attempts: number }>;
      }),
    );

    expect(deliveries).toHaveLength(1);
    expect(deliveries[0]?.status).toBe("failed");
    expect(deliveries[0]?.attempts).toBe(3);
  });
});
