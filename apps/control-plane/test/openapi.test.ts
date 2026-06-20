//! The control plane serves an OpenAPI 3.1 document whose schemas are derived from the same Effect
//! Schemas the routes validate with and the repositories return — named under `components/schemas` and
//! referenced by `$ref`, so client codegen produces named types and the contract can't drift.

import { readFileSync } from "node:fs";

import { HttpClient } from "@effect/platform";
import { Effect } from "effect";
import { describe, expect, it } from "vitest";

import { openApiDocument } from "../src/http/openapi";
import { freshDb, run } from "./support";

interface JsonSchema {
  readonly $ref?: string;
  readonly type?: string;
  readonly items?: { readonly $ref?: string };
  readonly properties?: Record<string, unknown>;
}

interface OpenApiDoc {
  readonly openapi: string;
  readonly info: { readonly title: string };
  readonly components: { readonly schemas: Record<string, JsonSchema> };
  readonly paths: Record<
    string,
    Record<
      string,
      {
        readonly requestBody?: {
          readonly content: { readonly "application/json": { readonly schema: JsonSchema } };
        };
        readonly responses?: Record<
          string,
          { readonly content?: { readonly "application/json": { readonly schema: JsonSchema } } }
        >;
      }
    >
  >;
}

describe("openapi", () => {
  it("serves a $ref'd OpenAPI 3.1 doc with named component schemas", async () => {
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

    // Named component schemas, derived from the route/repository Effect Schemas.
    const schemas = doc.components.schemas;
    expect(schemas.NewOrganization?.properties?.slug).toBeDefined();
    expect(schemas.Organization?.properties?.defaultCurrency).toBeDefined();
    expect(schemas.CreatedApiKey?.properties?.token).toBeDefined();
    expect(schemas.WebhookDelivery?.properties?.attempts).toBeDefined();

    // Operations reference the named schemas rather than inlining them.
    const orgPost = doc.paths["/v1/organizations"]?.post;
    expect(orgPost?.requestBody?.content["application/json"].schema.$ref).toBe(
      "#/components/schemas/NewOrganization",
    );

    // Lists are typed arrays of the resource schema.
    const orgList =
      doc.paths["/v1/organizations"]?.get?.responses?.["200"]?.content?.["application/json"].schema;
    expect(orgList?.type).toBe("array");
    expect(orgList?.items?.$ref).toBe("#/components/schemas/Organization");

    // Create endpoints answer 201 Created with the created resource.
    const created =
      doc.paths["/v1/api-keys"]?.post?.responses?.["201"]?.content?.["application/json"].schema;
    expect(created?.$ref).toBe("#/components/schemas/CreatedApiKey");
  });

  it("the committed openapi.json matches the served document (run openapi:emit if this fails)", () => {
    const committed = JSON.parse(readFileSync("openapi.json", "utf8"));
    expect(committed).toEqual(openApiDocument);
  });
});
