//! The active organization is part of the request context, not the URL. It lives in a cookie so it
//! survives reloads and deep links without forcing an `?org=` query param onto every page (the source
//! of the "org switcher can't load this page" bug). Reads are server-only; the switcher writes it via
//! a server action.

import { cookies } from "next/headers"

export const ACTIVE_ORG_COOKIE = "meter_active_org"

const ONE_YEAR_SECONDS = 60 * 60 * 24 * 365

/** The org id the operator last selected, if any. */
export async function readActiveOrgId(): Promise<string | undefined> {
  const store = await cookies()
  return store.get(ACTIVE_ORG_COOKIE)?.value
}

/** Persist the selected org. Called from the org-switcher server action. */
export async function writeActiveOrgId(orgId: string): Promise<void> {
  const store = await cookies()
  store.set(ACTIVE_ORG_COOKIE, orgId, {
    httpOnly: true,
    sameSite: "lax",
    secure: true,
    path: "/",
    maxAge: ONE_YEAR_SECONDS,
  })
}
