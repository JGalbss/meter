//! Products repository — Effect-wrapped Drizzle queries with a typed error channel.

import { eq } from "drizzle-orm";
import { Effect, Schema } from "effect";

import type { Db } from "../db/client";
import { products } from "../db/schema";
import { RepoError } from "../repository/errors";

// The response Schema is the single source of truth for the `Product` type + the OpenAPI contract.
export const Product = Schema.Struct({
  id: Schema.String,
  orgId: Schema.String,
  key: Schema.String,
  name: Schema.String,
});
export type Product = typeof Product.Type;

export interface NewProduct {
  readonly orgId: string;
  readonly key: string;
  readonly name: string;
}

function toProduct(row: typeof products.$inferSelect): Product {
  return { id: row.id, orgId: row.orgId, key: row.key, name: row.name };
}

/** Create a product. Unique per (org, key). */
export function createProduct(db: Db, input: NewProduct): Effect.Effect<Product, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db.insert(products).values(input).returning();
      if (row === undefined) {
        throw new Error("insert returned no row");
      }
      return toProduct(row);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** List an organization's products. */
export function listProducts(db: Db, orgId: string): Effect.Effect<readonly Product[], RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const rows = await db.select().from(products).where(eq(products.orgId, orgId));
      return rows.map(toProduct);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}
