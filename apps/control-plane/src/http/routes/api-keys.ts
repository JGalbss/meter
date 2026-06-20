//! API-key routes: mint (token shown once), list (never the token), and revoke.

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect, Schema } from "effect";

import { createApiKey, listApiKeys, revokeApiKey } from "../../api-keys/repository";
import { Database } from "../../db/service";
import { handle } from "../errors";

export const NewApiKeyBody = Schema.Struct({
  orgId: Schema.String,
  name: Schema.String,
  role: Schema.optional(Schema.Literal("viewer", "member", "admin")),
});
const OrgQuery = Schema.Struct({ orgId: Schema.String });
const IdParam = Schema.Struct({ id: Schema.String });

export function apiKeyRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database> {
  return base.pipe(
    HttpRouter.get(
      "/v1/api-keys",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const keys = yield* listApiKeys(db, orgId);
          return HttpServerResponse.unsafeJson(keys);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/api-keys",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const body = yield* HttpServerRequest.schemaBodyJson(NewApiKeyBody);
          const created = yield* createApiKey(db, body);
          return HttpServerResponse.unsafeJson(created, { status: 201 });
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/api-keys/:id/revoke",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const { id } = yield* HttpRouter.schemaPathParams(IdParam);
          const key = yield* revokeApiKey(db, id, new Date());
          return HttpServerResponse.unsafeJson(key);
        }),
      ),
    ),
  );
}
