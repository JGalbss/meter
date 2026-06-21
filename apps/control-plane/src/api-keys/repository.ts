//! API-keys repository. Tokens are random; only their SHA-256 hash is stored. The plaintext is
//! returned exactly once (at creation). Verification hashes the presented token and looks it up.

import { createHash, randomBytes } from "node:crypto";

import { and, desc, eq, isNull } from "drizzle-orm";
import { Effect, Schema } from "effect";

import type { Db } from "../db/client";
import { apiKeys } from "../db/schema";
import { type Role, toRole } from "../http/rbac";
import { type Principal, type Scope, toScope } from "../http/tenant";
import { NotFound, RepoError } from "../repository/errors";

export type { Principal } from "../http/tenant";

// The response Schema is the single source of truth for the `ApiKey` type + the OpenAPI contract.
export const ApiKey = Schema.Struct({
  id: Schema.String,
  orgId: Schema.String,
  name: Schema.String,
  role: Schema.Literal("viewer", "member", "admin"),
  scope: Schema.Literal("platform", "org"),
  prefix: Schema.String,
  createdAt: Schema.String,
  lastUsedAt: Schema.NullOr(Schema.String),
  revokedAt: Schema.NullOr(Schema.String),
});
export type ApiKey = typeof ApiKey.Type;

/** A freshly created key — the only time the plaintext token is available. */
export const CreatedApiKey = Schema.extend(ApiKey, Schema.Struct({ token: Schema.String }));
export type CreatedApiKey = typeof CreatedApiKey.Type;

export interface NewApiKey {
  readonly orgId: string;
  readonly name: string;
  readonly role?: Role | undefined;
  readonly scope?: Scope | undefined;
}

function hashToken(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}

function isoOrNull(at: Date | null): string | null {
  if (at === null) {
    return null;
  }
  return at.toISOString();
}

function toApiKey(row: typeof apiKeys.$inferSelect): ApiKey {
  return {
    id: row.id,
    orgId: row.orgId,
    name: row.name,
    role: toRole(row.role),
    scope: toScope(row.scope),
    prefix: row.prefix,
    createdAt: row.createdAt.toISOString(),
    lastUsedAt: isoOrNull(row.lastUsedAt),
    revokedAt: isoOrNull(row.revokedAt),
  };
}

function requireRow<A>(row: A | undefined, id: string): Effect.Effect<A, NotFound> {
  if (row === undefined) {
    return Effect.fail(new NotFound({ resource: "api_key", id }));
  }
  return Effect.succeed(row);
}

/** Mint an API key. Returns the plaintext token once; only its hash is persisted. */
export function createApiKey(db: Db, input: NewApiKey): Effect.Effect<CreatedApiKey, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const token = `mk_${randomBytes(24).toString("base64url")}`;
      const prefix = token.slice(0, 11);
      const [row] = await db
        .insert(apiKeys)
        .values({
          orgId: input.orgId,
          name: input.name,
          role: input.role ?? "admin",
          scope: input.scope ?? "org",
          prefix,
          tokenHash: hashToken(token),
        })
        .returning();
      if (row === undefined) {
        throw new Error("insert returned no row");
      }
      return { ...toApiKey(row), token };
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** List an organization's API keys (never the token or its hash). */
export function listApiKeys(db: Db, orgId: string): Effect.Effect<readonly ApiKey[], RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const rows = await db
        .select()
        .from(apiKeys)
        .where(eq(apiKeys.orgId, orgId))
        .orderBy(desc(apiKeys.createdAt));
      return rows.map(toApiKey);
    },
    catch: (cause) => new RepoError({ cause }),
  });
}

/** Revoke an API key. */
export function revokeApiKey(
  db: Db,
  id: string,
  now: Date,
): Effect.Effect<ApiKey, RepoError | NotFound> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db
        .update(apiKeys)
        .set({ revokedAt: now })
        .where(eq(apiKeys.id, id))
        .returning();
      return row;
    },
    catch: (cause) => new RepoError({ cause }),
  }).pipe(
    Effect.flatMap((row) => requireRow(row, id)),
    Effect.map(toApiKey),
  );
}

/** Resolve a presented token to its principal (org + role), or null if unknown/revoked. Stamps
 * `last_used_at`. */
export function verifyApiKey(db: Db, token: string): Effect.Effect<Principal | null, RepoError> {
  return Effect.tryPromise({
    try: async () => {
      const [row] = await db
        .select()
        .from(apiKeys)
        .where(and(eq(apiKeys.tokenHash, hashToken(token)), isNull(apiKeys.revokedAt)))
        .limit(1);
      if (row === undefined) {
        return null;
      }
      await db.update(apiKeys).set({ lastUsedAt: new Date() }).where(eq(apiKeys.id, row.id));
      return { orgId: row.orgId, role: toRole(row.role), scope: toScope(row.scope) };
    },
    catch: (cause) => new RepoError({ cause }),
  });
}
