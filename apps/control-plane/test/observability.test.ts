//! Observability middleware: every response carries an `x-request-id`, and a caller-supplied one is
//! propagated (echoed) so requests can be correlated across the dashboard, control plane, and logs.

import { HttpClient, HttpClientRequest, HttpServer } from "@effect/platform";
import { NodeHttpServer } from "@effect/platform-node";
import { Effect, Layer } from "effect";
import { describe, expect, it } from "vitest";

import { Database } from "../src/db/service";
import { withObservability } from "../src/http/observability";
import { router } from "../src/http/router";
import { type TestDb, freshDb } from "./support";

function observedLayer(db: TestDb) {
  return HttpServer.serve(withObservability(router)).pipe(
    Layer.provide(Layer.succeed(Database, db)),
    Layer.provideMerge(NodeHttpServer.layerTest),
  );
}

describe("observability", () => {
  it("returns an x-request-id and echoes a caller-supplied one", async () => {
    const db = await freshDb();
    const result = await Effect.gen(function* () {
      const client = yield* HttpClient.HttpClient;
      const generated = yield* client.get("/health");
      const echoed = yield* client.execute(
        HttpClientRequest.get("/health").pipe(
          HttpClientRequest.setHeader("x-request-id", "corr-abc-123"),
        ),
      );
      return {
        status: generated.status,
        generated: generated.headers["x-request-id"],
        echoed: echoed.headers["x-request-id"],
      };
    }).pipe(Effect.scoped, Effect.provide(observedLayer(db)), Effect.runPromise);

    expect(result.status).toBe(200);
    expect(result.generated).toBeDefined();
    expect(result.generated).not.toBe("");
    expect(result.echoed).toBe("corr-abc-123");
  });
});
