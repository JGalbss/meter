import { PGlite } from "@electric-sql/pglite";
import { drizzle } from "drizzle-orm/pglite";
import { migrate } from "drizzle-orm/pglite/migrator";
import { Effect } from "effect";
import { describe, expect, it } from "vitest";

import { createAgent, listAgents } from "../src/agents/repository";
import * as schema from "../src/db/schema";

async function freshDb() {
  const db = drizzle(new PGlite(), { schema });
  await migrate(db, { migrationsFolder: "./drizzle" });
  return db;
}

describe("agents repository", () => {
  it("creates and lists agents scoped to an org", async () => {
    const db = await freshDb();
    const [org] = await db
      .insert(schema.organizations)
      .values({ slug: "acme", name: "Acme" })
      .returning();
    if (org === undefined) {
      throw new Error("failed to seed org");
    }

    const created = await Effect.runPromise(
      createAgent(db, { orgId: org.id, key: "chat", name: "Chat" }),
    );
    expect(created.key).toBe("chat");
    expect(created.orgId).toBe(org.id);

    const listed = await Effect.runPromise(listAgents(db, org.id));
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

    await Effect.runPromise(createAgent(db, { orgId: org.id, key: "chat", name: "Chat" }));
    const duplicate = await Effect.runPromiseExit(
      createAgent(db, { orgId: org.id, key: "chat", name: "Chat again" }),
    );
    expect(duplicate._tag).toBe("Failure");
  });
});
