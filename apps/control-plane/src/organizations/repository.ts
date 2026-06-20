//! Organizations repository — Effect-wrapped Drizzle queries with a typed error channel.

import { Effect } from "effect";

import type { Db } from "../db/client";
import { organizations } from "../db/schema";
import { RepoError } from "../repository/errors";

export interface Organization {
  readonly id: string;
  readonly slug: string;
  readonly name: string;
  readonly defaultCurrency: string;
}

export interface NewOrganization {
  readonly slug: string;
  readonly name: string;
}

function toOrganization(row: typeof organizations.$inferSelect): Organization {
  return { id: row.id, slug: row.slug, name: row.name, defaultCurrency: row.defaultCurrency };
}

/** Create an organization. Slug is unique. */
export function createOrganization(
  db: Db,
  input: NewOrganization,
): Effect.Effect<Organization, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db.insert(organizations).values(input).returning();
      if (row === undefined) {
        throw new Error("insert returned no row");
      }
      return toOrganization(row);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** List all organizations. */
export function listOrganizations(db: Db): Effect.Effect<readonly Organization[], RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const rows = await db.select().from(organizations);
      return rows.map(toOrganization);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}
