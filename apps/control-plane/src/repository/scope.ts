//! Shared tenant-scoping for by-id mutations. A row is matched by id and, when the caller is confined
//! to an organization, additionally by org — so an org-scoped key cannot mutate another org's row (the
//! update simply matches nothing and the handler returns 404). A `null` org means unconfined (platform
//! keys, dev no-auth). See ADR 0007.

import { type Column, type SQL, and, eq } from "drizzle-orm";

/** WHERE clause matching a row by id, optionally confined to an org (`null` = unconfined). */
export function byIdInOrg(
  idColumn: Column,
  orgColumn: Column,
  id: string,
  orgId: string | null,
): SQL {
  if (orgId === null) {
    return eq(idColumn, id);
  }
  const clause = and(eq(idColumn, id), eq(orgColumn, orgId));
  if (clause === undefined) {
    return eq(idColumn, id);
  }
  return clause;
}
