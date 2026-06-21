//! Agent routes: create and list (scoped to an organization).

import { HttpRouter, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect, Schema } from "effect";

import { createAgent, listAgents } from "../../agents/repository";
import { Database } from "../../db/service";
import { forbidden, handle } from "../errors";
import { CurrentPrincipal, authorizeOrg, isAllowed } from "../tenant";

export const NewAgentBody = Schema.Struct({
  orgId: Schema.String,
  key: Schema.String,
  name: Schema.String,
});

const AgentQuery = Schema.Struct({ orgId: Schema.String });

export function agentRoutes<E, R>(
  base: HttpRouter.HttpRouter<E, R>,
): HttpRouter.HttpRouter<E, R | Database | CurrentPrincipal> {
  return base.pipe(
    HttpRouter.get(
      "/v1/agents",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const { orgId } = yield* HttpServerRequest.schemaSearchParams(AgentQuery);
          const access = authorizeOrg(principal, orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const items = yield* listAgents(db, access.orgId);
          return HttpServerResponse.unsafeJson(items);
        }),
      ),
    ),
    HttpRouter.post(
      "/v1/agents",
      handle(
        Effect.gen(function* () {
          const db = yield* Database;
          const principal = yield* CurrentPrincipal;
          const body = yield* HttpServerRequest.schemaBodyJson(NewAgentBody);
          const access = authorizeOrg(principal, body.orgId);
          if (!isAllowed(access)) {
            return forbidden;
          }
          const agent = yield* createAgent(db, { ...body, orgId: access.orgId });
          return HttpServerResponse.unsafeJson(agent, { status: 201 });
        }),
      ),
    ),
  );
}
