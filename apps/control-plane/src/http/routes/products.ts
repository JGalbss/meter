//! Product routes: create and list (scoped to an organization).

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect, Schema } from "effect";

import { Database } from "../../db/service";
import { createProduct, listProducts } from "../../products/repository";
import { forbidden, handle } from "../errors";
import { CurrentPrincipal, authorizeOrg, isAllowed } from "../tenant";

export const NewProductBody = Schema.Struct({
  orgId: Schema.String,
  key: Schema.String,
  name: Schema.String,
});

const ProductQuery = Schema.Struct({ orgId: Schema.String });

export function productRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database | CurrentPrincipal> {
  return base.pipe(
    HttpRouter.get(
      "/v1/products",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(ProductQuery);
          const access = authorizeOrg(principal, orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const items = yield* listProducts(db, access.orgId);
          return HttpServerResponse.unsafeJson(items);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/products",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const body = yield* HttpServerRequest.schemaBodyJson(NewProductBody);
          const access = authorizeOrg(principal, body.orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const product = yield* createProduct(db, { ...body, orgId: access.orgId });
          return HttpServerResponse.unsafeJson(product, { status: 201 });
        }),
      ),
    ),
  );
}
