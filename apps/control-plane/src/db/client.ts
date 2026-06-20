//! Database client. Production uses the postgres-js driver; tests use PGlite. Both satisfy [`Db`].

import type { PgliteDatabase } from "drizzle-orm/pglite";
import { drizzle } from "drizzle-orm/postgres-js";
import type { PostgresJsDatabase } from "drizzle-orm/postgres-js";
import postgres from "postgres";

import * as schema from "./schema";

/** A Drizzle database over the control-plane schema, on either driver. */
export type Db = PostgresJsDatabase<typeof schema> | PgliteDatabase<typeof schema>;

/** Open a production database connection. */
export function makeDb(connectionString: string): PostgresJsDatabase<typeof schema> {
  const client = postgres(connectionString);
  return drizzle(client, { schema });
}
