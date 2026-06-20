//! Resolve the active organization for org-scoped pages: list orgs, then pick the one requested via
//! `?org=`, falling back to the first. Surfaces a control-plane error so pages can degrade gracefully.

import { listOrganizations } from "./client";
import type { Organization } from "./types";

export interface OrgScope {
  readonly orgs: readonly Organization[];
  readonly activeOrg: Organization | null;
  readonly error: string | null;
}

function pickOrg(orgs: readonly Organization[], requested: string | undefined): Organization | null {
  if (requested !== undefined) {
    const match = orgs.find((org) => org.id === requested);
    if (match !== undefined) {
      return match;
    }
  }
  return orgs[0] ?? null;
}

export async function resolveOrgScope(requested?: string): Promise<OrgScope> {
  const result = await listOrganizations();
  if (!result.ok) {
    return { orgs: [], activeOrg: null, error: result.error };
  }
  return { orgs: result.data, activeOrg: pickOrg(result.data, requested), error: null };
}
