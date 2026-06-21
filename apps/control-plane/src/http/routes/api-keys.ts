//! API-key routes: mint (token shown once), list (never the token), and revoke.

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Clock, Effect, Schema } from "effect";

import { createApiKey, listApiKeys, revokeApiKey } from "../../api-keys/repository";
import { Database } from "../../db/service";
import { forbidden, handle } from "../errors";
import { CurrentPrincipal, authorizeOrg, canMintScope, isAllowed, orgScope } from "../tenant";

export const NewApiKeyBody = Schema.Struct({
  orgId: Schema.String,
  name: Schema.String,
  role: Schema.optional(Schema.Literal("viewer", "member", "admin")),
  scope: Schema.optional(Schema.Literal("platform", "org")),
});
const OrgQuery = Schema.Struct({ orgId: Schema.String });
const IdParam = Schema.Struct({ id: Schema.String });

export function apiKeyRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database | CurrentPrincipal> {
  return base.pipe(
    HttpRouter.get(
      "/v1/api-keys",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(OrgQuery);
          const access = authorizeOrg(principal, orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const keys = yield* listApiKeys(db, access.orgId);
          return HttpServerResponse.unsafeJson(keys);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/api-keys",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const body = yield* HttpServerRequest.schemaBodyJson(NewApiKeyBody);
          const access = authorizeOrg(principal, body.orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const scope = body.scope ?? "org";
          if (!canMintScope(principal, scope)) {
            return forbidden;
          }
          const created = yield* createApiKey(db, { ...body, orgId: access.orgId, scope });
          return HttpServerResponse.unsafeJson(created, { status: 201 });
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/api-keys/:id/revoke",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { id } = yield* HttpRouter.schemaPathParams(IdParam);
          const now = new Date(yield* Clock.currentTimeMillis);
          const key = yield* revokeApiKey(db, id, orgScope(principal), now);
          return HttpServerResponse.unsafeJson(key);
        }),
      ),
    ),
  );
}
