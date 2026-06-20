//! The control plane serves an OpenAPI 3.1 document whose request bodies are derived from the same
//! Effect Schemas the routes validate with (so the contract cannot drift from validation).

import { HttpClient } from "@effect/platform";
import { Effect } from "effect";
import { describe, expect, it } from "vitest";

import { freshDb, run } from "./support";

interface OpenApiDoc {
  readonly openapi: string;
  readonly info: { readonly title: string };
  readonly paths: Record<
    string,
    Record<
      string,
      {
        readonly requestBody?: {
          readonly content: {
            readonly "application/json": {
              readonly schema: { readonly properties?: Record<string, unknown> };
            };
          };
        };
        readonly responses?: Record<
          string,
          {
            readonly content?: {
              readonly "application/json": {
                readonly schema: {
                  readonly type?: string;
                  readonly items?: { readonly properties?: Record<string, unknown> };
                  readonly properties?: Record<string, unknown>;
                };
              };
            };
          }
        >;
      }
    >
  >;
}

describe("openapi", () => {
  it("serves an OpenAPI 3.1 doc with request schemas derived from the route Schemas", async () => {
    const db = await freshDb();
    const doc = (await run(
      db,
      Effect.gen(function* () {
        const client = yield* HttpClient.HttpClient;
        const response = yield* client.get("/openapi.json");
        return yield* response.json;
      }),
    )) as OpenApiDoc;

    expect(doc.openapi).toBe("3.1.0");
    expect(doc.info.title).toBe("meter control plane");

    // Every resource is present.
    for (const path of [
      "/v1/organizations",
      "/v1/products",
      "/v1/api-keys",
      "/v1/alert-rules",
      "/v1/notifications",
      "/v1/webhooks",
    ]) {
      expect(doc.paths[path]).toBeDefined();
    }

    // A parameterized route is documented with templated syntax.
    expect(doc.paths["/v1/api-keys/{id}/revoke"]?.post).toBeDefined();

    // The create-organization body schema is derived from NewOrganizationBody (slug + name).
    const orgBody =
      doc.paths["/v1/organizations"]?.post?.requestBody?.content["application/json"].schema;
    expect(orgBody?.properties?.slug).toBeDefined();
    expect(orgBody?.properties?.name).toBeDefined();

    // List responses are typed arrays of the resource Schema (Organization → id/slug/name/...).
    const orgList =
      doc.paths["/v1/organizations"]?.get?.responses?.["200"]?.content?.["application/json"].schema;
    expect(orgList?.type).toBe("array");
    expect(orgList?.items?.properties?.defaultCurrency).toBeDefined();

    // Create endpoints answer 201 Created; minting a key returns CreatedApiKey with the one-time token.
    const created =
      doc.paths["/v1/api-keys"]?.post?.responses?.["201"]?.content?.["application/json"].schema;
    expect(created?.properties?.token).toBeDefined();
    expect(created?.properties?.role).toBeDefined();

    // The remaining resources are typed too (webhook deliveries → array of the delivery Schema).
    const deliveries =
      doc.paths["/v1/webhook-deliveries"]?.get?.responses?.["200"]?.content?.["application/json"]
        .schema;
    expect(deliveries?.type).toBe("array");
    expect(deliveries?.items?.properties?.attempts).toBeDefined();
  });
});
