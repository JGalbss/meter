//! End-to-end test for budget alert evaluation: the control plane queries a stubbed engine for budget
//! status and, on escalation, raises a notification and fires the matching webhook (with dead-letter
//! safe dedup so a sustained breach does not spam).

import { createServer } from "node:http";

import { HttpClient, HttpClientRequest } from "@effect/platform";
import { Effect } from "effect";
import { afterEach, describe, expect, it } from "vitest";

import { evaluateAllOrgs } from "../src/alerts/evaluate";
import { freshDb, run } from "./support";

interface Sink {
  readonly url: string;
  readonly received: string[];
  readonly close: () => Promise<void>;
}

async function startSink(respond: () => { status: number; body: string }): Promise<Sink> {
  const received: string[] = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      received.push(body);
      const { status, body: payload } = respond();
      res.writeHead(status, { "content-type": "application/json" });
      res.end(payload);
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error("sink has no port");
  }
  return {
    url: `http://127.0.0.1:${address.port}`,
    received,
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  };
}

function budgetBody(status: string, ratio: string, used: string): string {
  return JSON.stringify({
    used_credits: used,
    limit_credits: "1000",
    remaining_credits: "0",
    ratio,
    status,
  });
}

const servers: Sink[] = [];
async function sink(respond: () => { status: number; body: string }): Promise<Sink> {
  const created = await startSink(respond);
  servers.push(created);
  return created;
}

afterEach(async () => {
  await Promise.all(servers.splice(0).map((s) => s.close()));
  Reflect.deleteProperty(process.env, "METER_ENGINE_URL");
});

const ACCOUNT = "00000000-0000-0000-0000-0000000000aa";

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

function createBudgetRule(client: HttpClient.HttpClient, orgId: string) {
  return client.execute(
    HttpClientRequest.post("/v1/alert-rules").pipe(
      HttpClientRequest.bodyUnsafeJson({
        orgId,
        name: "Monthly cap",
        scope: "org",
        metric: "budget",
        threshold: 0.8,
        action: "notify",
        accountId: ACCOUNT,
        creditLimit: 1000,
        windowDays: 30,
      }),
    ),
  );
}

describe("budget alert evaluation", () => {
  it("raises a notification and fires a webhook on escalation, then dedups", async () => {
    const engine = await sink(() => ({ status: 200, body: budgetBody("exceeded", "1.2", "1200") }));
    const webhook = await sink(() => ({ status: 200, body: "" }));
    process.env.METER_ENGINE_URL = engine.url;

    const db = await freshDb();
    const result = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);

        yield* client.execute(
          HttpClientRequest.post("/v1/webhooks").pipe(
            HttpClientRequest.bodyUnsafeJson({
              orgId: org.id,
              url: webhook.url,
              secret: "shh",
              eventTypes: ["budget"],
            }),
          ),
        );
        yield* createBudgetRule(client, org.id);

        const first = yield* client.execute(
          HttpClientRequest.post(`/v1/alert-rules/evaluate?orgId=${org.id}`),
        );
        const firstSummary = (yield* first.json) as { evaluated: number; raised: number };

        const second = yield* client.execute(
          HttpClientRequest.post(`/v1/alert-rules/evaluate?orgId=${org.id}`),
        );
        const secondSummary = (yield* second.json) as { evaluated: number; raised: number };

        const notes = yield* client.get(`/v1/notifications?orgId=${org.id}`);
        const notifications = (yield* notes.json) as ReadonlyArray<{
          type: string;
          severity: string;
        }>;

        return { firstSummary, secondSummary, notifications };
      }),
    );

    expect(result.firstSummary).toEqual({ evaluated: 1, raised: 1 });
    expect(result.secondSummary).toEqual({ evaluated: 1, raised: 0 });
    expect(result.notifications).toHaveLength(1);
    expect(result.notifications[0]).toMatchObject({ type: "budget", severity: "critical" });
    expect(webhook.received).toHaveLength(1);
  });

  it("evaluates budget rules across every organization (scheduler unit of work)", async () => {
    const engine = await sink(() => ({ status: 200, body: budgetBody("exceeded", "1.2", "1200") }));
    process.env.METER_ENGINE_URL = engine.url;

    const db = await freshDb();
    await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        for (const slug of ["one", "two"]) {
          const created = yield* client.execute(
            HttpClientRequest.post("/v1/organizations").pipe(
              HttpClientRequest.bodyUnsafeJson({ slug, name: slug }),
            ),
          );
          const org = (yield* created.json) as { id: string };
          yield* createBudgetRule(client, org.id);
        }
      }),
    );

    const summary = await Effect.runPromise(evaluateAllOrgs(db));
    expect(summary.orgs).toBe(2);
    expect(summary.raised).toBe(2);
  });

  it("does not alert while the budget is healthy", async () => {
    const engine = await sink(() => ({ status: 200, body: budgetBody("ok", "0.10", "100") }));
    process.env.METER_ENGINE_URL = engine.url;

    const db = await freshDb();
    const summary = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);
        yield* createBudgetRule(client, org.id);
        const response = yield* client.execute(
          HttpClientRequest.post(`/v1/alert-rules/evaluate?orgId=${org.id}`),
        );
        return (yield* response.json) as { evaluated: number; raised: number };
      }),
    );

    expect(summary).toEqual({ evaluated: 1, raised: 0 });
  });

  it("degrades gracefully when the engine is unreachable", async () => {
    const closed = await startSink(() => ({ status: 200, body: "{}" }));
    process.env.METER_ENGINE_URL = closed.url;
    await closed.close();

    const db = await freshDb();
    const summary = await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const org = yield* seedOrg(client);
        yield* createBudgetRule(client, org.id);
        const response = yield* client.execute(
          HttpClientRequest.post(`/v1/alert-rules/evaluate?orgId=${org.id}`),
        );
        return (yield* response.json) as { evaluated: number; raised: number };
      }),
    );

    expect(summary.raised).toBe(0);
  });
});
