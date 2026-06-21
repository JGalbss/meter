//! Agents repository — Effect-wrapped Drizzle queries with a typed error channel.

import { eq } from "drizzle-orm";
import { Effect, Schema } from "effect";

import type { Db } from "../db/client";
import { agents } from "../db/schema";
import { RepoError } from "../repository/errors";

// The response Schema is the single source of truth for the `Agent` type + the OpenAPI contract.
export const Agent = Schema.Struct({
  id: Schema.String,
  orgId: Schema.String,
  key: Schema.String,
  name: Schema.String,
});
export type Agent = typeof Agent.Type;

export interface NewAgent {
  readonly orgId: string;
  readonly key: string;
  readonly name: string;
}

function toAgent(row: typeof agents.$inferSelect): Agent {
  return { id: row.id, orgId: row.orgId, key: row.key, name: row.name };
}

/** Create an agent. Unique per (org, key). */
export function createAgent(db: Db, input: NewAgent): Effect.Effect<Agent, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db.insert(agents).values(input).returning();
      if (row === undefined) {
        throw new Error("insert returned no row");
      }
      return toAgent(row);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** List an organization's agents. */
export function listAgents(db: Db, orgId: string): Effect.Effect<readonly Agent[], RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const rows = await db.select().from(agents).where(eq(agents.orgId, orgId));
      return rows.map(toAgent);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}
