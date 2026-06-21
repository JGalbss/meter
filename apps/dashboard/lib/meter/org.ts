//! Resolve the active organization for org-scoped pages. The selected org lives in a cookie (request
//! context), so pages never need an `?org=` query param. Precedence: the cookie's org if it still
//! exists, otherwise the first org. Surfaces a control-plane error so pages can degrade gracefully
//! instead of rendering a misleading "no organizations".

import { readActiveOrgId } from "./active-org"
import { listOrganizations } from "./client"
import type { Organization } from "./types"

export interface OrgScope {
  readonly orgs: readonly Organization[]
  readonly activeOrg: Organization | null
  readonly error: string | null
}

function pickOrg(
  orgs: readonly Organization[],
  preferred: string | undefined
): Organization | null {
  if (preferred !== undefined) {
    const match = orgs.find((org) => org.id === preferred)
    if (match !== undefined) {
      return match
    }
  }
  return orgs[0] ?? null
}

export async function resolveOrgScope(): Promise<OrgScope> {
  const result = await listOrganizations()
  if (!result.ok) {
    return { orgs: [], activeOrg: null, error: result.error }
  }
  const preferred = await readActiveOrgId()
  return {
    orgs: result.data,
    activeOrg: pickOrg(result.data, preferred),
    error: null,
  }
}
