//! HTTP router: the control-plane CRUD surface the dashboard hits. Config only — money lives in the
//! engine. Each handler validates input with `Schema`, talks to a repository, and maps typed
//! failures to JSON responses. No money is ever computed here.

import type { HttpServerError } from "@effect/platform";
import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import type { ParseResult } from "effect";
import { Effect, Match, Schema } from "effect";

import { Database } from "../db/service";
import type { RepoError as OrgRepoError } from "../organizations/repository";
import { createOrganization, listOrganizations } from "../organizations/repository";
import type { RepoError as ProductRepoError } from "../products/repository";
import { createProduct, listProducts } from "../products/repository";

/** Every failure a handler can surface, normalized to a JSON response by [`errorResponse`]. */
type AppError =
  | OrgRepoError
  | ProductRepoError
  | ParseResult.ParseError
  | HttpServerError.RequestError;

const NewOrganizationBody = Schema.Struct({
  slug: Schema.String,
  name: Schema.String,
});

const NewProductBody = Schema.Struct({
  orgId: Schema.String,
  key: Schema.String,
  name: Schema.String,
});

const ProductQuery = Schema.Struct({ orgId: Schema.String });

/** Map a typed failure to a clean JSON response. Repo failures are internal; the rest are bad input.
 * A response is itself a yieldable `Effect`, so this is usable directly as a `catchAll` recovery. */
function errorResponse(error: AppError): Effect.Effect<HttpServerResponse.HttpServerResponse> {
  return Match.value(error).pipe(
    Match.tag("RepoError", () =>
      HttpServerResponse.unsafeJson({ error: "internal" }, { status: 500 }),
    ),
    Match.tag("ParseError", (parse) =>
      HttpServerResponse.unsafeJson({ error: "invalid", detail: parse.message }, { status: 400 }),
    ),
    Match.tag("RequestError", () =>
      HttpServerResponse.unsafeJson({ error: "invalid_request" }, { status: 400 }),
    ),
    Match.orElse(() => HttpServerResponse.unsafeJson({ error: "internal" }, { status: 500 })),
  );
}

/** Run a handler, turning any typed failure into a JSON response so the route never rejects. */
function handle<R>(
  effect: Effect.Effect<HttpServerResponse.HttpServerResponse, AppError, R>,
): Effect.Effect<HttpServerResponse.HttpServerResponse, never, R> {
  return effect.pipe(Effect.catchAll(errorResponse));
}

export const router: HttpRouter.HttpRouter<never, Database> = HttpRouter.empty.pipe(
  HttpRouter.get("/health", Effect.succeed(HttpServerResponse.unsafeJson({ status: "ok" }))),
  HttpRouter.get(
    "/v1/organizations",
    handle(
      Effect.gen(function* () {
        const db = yield* Database;
        const orgs = yield* listOrganizations(db);
        return HttpServerResponse.unsafeJson(orgs);
      }),
    ),
  ),
  HttpRouter.post(
    "/v1/organizations",
    handle(
      Effect.gen(function* () {
        const db = yield* Database;
        const body = yield* HttpServerRequest.schemaBodyJson(NewOrganizationBody);
        const org = yield* createOrganization(db, body);
        return HttpServerResponse.unsafeJson(org, { status: 201 });
      }),
    ),
  ),
  HttpRouter.get(
    "/v1/products",
    handle(
      Effect.gen(function* () {
        const db = yield* Database;
        const { orgId } = yield* HttpServerRequest.schemaSearchParams(ProductQuery);
        const items = yield* listProducts(db, orgId);
        return HttpServerResponse.unsafeJson(items);
      }),
    ),
  ),
  HttpRouter.post(
    "/v1/products",
    handle(
      Effect.gen(function* () {
        const db = yield* Database;
        const body = yield* HttpServerRequest.schemaBodyJson(NewProductBody);
        const product = yield* createProduct(db, body);
        return HttpServerResponse.unsafeJson(product, { status: 201 });
      }),
    ),
  ),
);
