//! Organization routes: create and list. Platform-scoped only — an org-scoped key may not enumerate or
//! create organizations (ADR 0007).

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect, Schema } from "effect";

import { Database } from "../../db/service";
import { createOrganization, listOrganizations } from "../../organizations/repository";
import { forbidden, handle } from "../errors";
import { CurrentPrincipal, canManagePlatform } from "../tenant";

export const NewOrganizationBody = Schema.Struct({
  slug: Schema.String,
  name: Schema.String,
});

export function organizationRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database | CurrentPrincipal> {
  return base.pipe(
    HttpRouter.get(
      "/v1/organizations",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          if (!canManagePlatform(principal)) {
            return forbidden;
          }
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
          const principal = yield* CurrentPrincipal;
          if (!canManagePlatform(principal)) {
            return forbidden;
          }
          const body = yield* HttpServerRequest.schemaBodyJson(NewOrganizationBody);
          const org = yield* createOrganization(db, body);
          return HttpServerResponse.unsafeJson(org, { status: 201 });
        }),
      ),
    ),
  );
}
