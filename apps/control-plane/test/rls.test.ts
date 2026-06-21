//! Row-Level Security enforcement, proven against a real Postgres (ADR 0007 defense-in-depth).
//!
//! PGlite runs as a superuser and so cannot enforce RLS, which is why the rest of the suite cannot
//! cover it. This suite is gated on `METER_TEST_PG_URL` (a superuser/owner connection string); CI
//! provides one. It applies the migrations, provisions the non-superuser application role the same way
//! deployment does, then connects AS that role and asserts the policies actually confine reads and
//! writes to the tenant named by the per-request `meter.org_id` setting — and fail closed when unset.

import { drizzle } from "drizzle-orm/postgres-js";
import { migrate } from "drizzle-orm/postgres-js/migrator";
import postgres from "postgres";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import * as schema from "../src/db/schema";

const superUrl = process.env.METER_TEST_PG_URL;
const APP_ROLE = "meter_app_test";
const APP_PASSWORD = "app";

const ORG_A = "11111111-1111-1111-1111-111111111111";
const ORG_B = "22222222-2222-2222-2222-222222222222";

function appUrlFrom(url: string): string {
  const parsed = new URL(url);
  parsed.username = APP_ROLE;
  parsed.password = APP_PASSWORD;
  return parsed.toString();
}

// Gated: only runs where a real Postgres is provided (PGlite can't enforce RLS).
describe.skipIf(!superUrl)("control-plane RLS enforcement (real Postgres)", () => {
  // biome-ignore lint/style/noNonNullAssertion: guarded by skipIf above.
  const url = superUrl!;
  let owner: postgres.Sql;
  let app: postgres.Sql;

  beforeAll(async () => {
    owner = postgres(url, { max: 4 });
    await migrate(drizzle(owner, { schema }), { migrationsFolder: "./drizzle" });

    // Provision the non-superuser application role (no BYPASSRLS) exactly as deployment does, then seed
    // two orgs' worth of data as the owner (superuser bypasses RLS, so the seed is unconstrained).
    await owner.unsafe(`
      DO $$ BEGIN
        IF EXISTS (SELECT FROM pg_roles WHERE rolname = '${APP_ROLE}') THEN
          EXECUTE 'DROP OWNED BY ${APP_ROLE}';
          EXECUTE 'DROP ROLE ${APP_ROLE}';
        END IF;
      END $$;
      CREATE ROLE ${APP_ROLE} LOGIN PASSWORD '${APP_PASSWORD}';
      GRANT USAGE ON SCHEMA public TO ${APP_ROLE};
      GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO ${APP_ROLE};
      GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO ${APP_ROLE};
    `);

    await owner.unsafe(`
      INSERT INTO organizations (id, slug, name) VALUES
        ('${ORG_A}', 'org-a', 'Org A'),
        ('${ORG_B}', 'org-b', 'Org B')
      ON CONFLICT (id) DO NOTHING;
      INSERT INTO products (org_id, key, name) VALUES
        ('${ORG_A}', 'p-a', 'Product A'),
        ('${ORG_B}', 'p-b', 'Product B')
      ON CONFLICT DO NOTHING;
    `);

    app = postgres(appUrlFrom(url), { max: 4 });
  });

  afterAll(async () => {
    await app?.end({ timeout: 5 });
    if (owner) {
      await owner.unsafe(`
        DO $$ BEGIN
          IF EXISTS (SELECT FROM pg_roles WHERE rolname = '${APP_ROLE}') THEN
            EXECUTE 'DROP OWNED BY ${APP_ROLE}';
            EXECUTE 'DROP ROLE ${APP_ROLE}';
          END IF;
        END $$;
      `);
      await owner.end({ timeout: 5 });
    }
  });

  // Run a callback inside a transaction with the per-request tenant settings applied (LOCAL), exactly
  // as the request middleware does.
  function bypassFlag(on: boolean | undefined): string {
    if (on === true) {
      return "on";
    }
    return "";
  }

  async function asTenant<T>(
    settings: { orgId?: string; bypass?: boolean },
    fn: (tx: postgres.TransactionSql) => Promise<T>,
  ): Promise<T> {
    const result = await app.begin(async (tx) => {
      await tx`select set_config('meter.org_id', ${settings.orgId ?? ""}, true)`;
      await tx`select set_config('meter.rls_bypass', ${bypassFlag(settings.bypass)}, true)`;
      return fn(tx);
    });
    return result as T;
  }

  it("confines reads to the tenant named by meter.org_id", async () => {
    const aSlugs = await asTenant({ orgId: ORG_A }, (tx) => tx`select slug from organizations`);
    expect(aSlugs.map((r) => r.slug)).toEqual(["org-a"]);

    const aProducts = await asTenant({ orgId: ORG_A }, (tx) => tx`select key from products`);
    expect(aProducts.map((r) => r.key)).toEqual(["p-a"]);

    const bProducts = await asTenant({ orgId: ORG_B }, (tx) => tx`select key from products`);
    expect(bProducts.map((r) => r.key)).toEqual(["p-b"]);
  });

  it("fails closed when no tenant is set (sees nothing)", async () => {
    const rows = await asTenant({}, (tx) => tx`select id from organizations`);
    expect(rows).toHaveLength(0);
    const products = await asTenant({}, (tx) => tx`select id from products`);
    expect(products).toHaveLength(0);
  });

  it("a platform bypass sees every tenant", async () => {
    const all = await asTenant(
      { bypass: true },
      (tx) => tx`select slug from organizations order by slug`,
    );
    expect(all.map((r) => r.slug)).toEqual(["org-a", "org-b"]);
  });

  it("refuses to write a row into another tenant (WITH CHECK)", async () => {
    await expect(
      asTenant(
        { orgId: ORG_A },
        (tx) => tx`insert into products (org_id, key, name) values (${ORG_B}, 'sneaky', 'Sneaky')`,
      ),
    ).rejects.toThrow();

    // The cross-tenant write left no trace (verified with a bypass read).
    const sneaky = await asTenant(
      { bypass: true },
      (tx) => tx`select key from products where key = 'sneaky'`,
    );
    expect(sneaky).toHaveLength(0);
  });

  it("cannot update another tenant's row across the boundary", async () => {
    // Acting as Org A, an UPDATE targeting Org B's product matches no rows (the row is invisible).
    const updated = await asTenant(
      { orgId: ORG_A },
      (tx) => tx`update products set name = 'hijacked' where key = 'p-b' returning id`,
    );
    expect(updated).toHaveLength(0);
  });
});
