import { PGlite } from "@electric-sql/pglite";
import { drizzle } from "drizzle-orm/pglite";
import { migrate } from "drizzle-orm/pglite/migrator";
import { Effect } from "effect";
import { describe, expect, it } from "vitest";

import * as schema from "../src/db/schema";
import { createProduct, listProducts } from "../src/products/repository";

async function freshDb() {
  const db = drizzle(new PGlite(), { schema });
  await migrate(db, { migrationsFolder: "./drizzle" });
  return db;
}

describe("products repository", () => {
  it("creates and lists products scoped to an org", async () => {
    const db = await freshDb();
    const [org] = await db
      .insert(schema.organizations)
      .values({ slug: "acme", name: "Acme" })
      .returning();
    if (org === undefined) {
      throw new Error("failed to seed org");
    }

    const created = await Effect.runPromise(
      createProduct(db, { orgId: org.id, key: "chat", name: "Chat" }),
    );
    expect(created.key).toBe("chat");
    expect(created.orgId).toBe(org.id);

    const listed = await Effect.runPromise(listProducts(db, org.id));
    expect(listed).toHaveLength(1);
    expect(listed[0]?.name).toBe("Chat");
  });

  it("enforces unique (org, key)", async () => {
    const db = await freshDb();
    const [org] = await db
      .insert(schema.organizations)
      .values({ slug: "beta", name: "Beta" })
      .returning();
    if (org === undefined) {
      throw new Error("failed to seed org");
    }

    await Effect.runPromise(createProduct(db, { orgId: org.id, key: "chat", name: "Chat" }));
    const duplicate = await Effect.runPromiseExit(
      createProduct(db, { orgId: org.id, key: "chat", name: "Chat again" }),
    );
    expect(duplicate._tag).toBe("Failure");
  });
});
