//! First-run onboarding state. A fresh deployment lands the operator in a guided setup; once they
//! finish (or skip) we set a cookie so the dashboard stops routing them back to it.

import { cookies } from "next/headers"

const DISMISSED_COOKIE = "meter_onboarding_dismissed"
const ONE_YEAR_SECONDS = 60 * 60 * 24 * 365

/** Whether the operator has finished or skipped onboarding on this browser. */
export async function isOnboardingDismissed(): Promise<boolean> {
  const store = await cookies()
  return store.get(DISMISSED_COOKIE)?.value === "1"
}

/** Stop routing to onboarding (set when the flow completes or the operator skips). */
export async function dismissOnboarding(): Promise<void> {
  const store = await cookies()
  store.set(DISMISSED_COOKIE, "1", {
    httpOnly: true,
    sameSite: "lax",
    secure: true,
    path: "/",
    maxAge: ONE_YEAR_SECONDS,
  })
}
