//! Product routes: create and list (scoped to an organization).

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect, Schema } from "effect";

import { Database } from "../../db/service";
import { createProduct, listProducts } from "../../products/repository";
import { handle } from "../errors";

const NewProductBody = Schema.Struct({
  orgId: Schema.String,
  key: Schema.String,
  name: Schema.String,
});

const ProductQuery = Schema.Struct({ orgId: Schema.String });

export function productRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database> {
  return base.pipe(
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
}
