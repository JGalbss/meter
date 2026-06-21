"use server"

import { revalidatePath } from "next/cache"
import { redirect } from "next/navigation"

import { requireSession } from "@/lib/auth/session"
import { endSession } from "@/lib/auth/session"
import { writeActiveOrgId } from "@/lib/meter/active-org"

// Intentionally session-less: clearing your own session is safe for anyone to call, and gating logout
// behind a valid session would strand expired sessions. (Accepted react-doctor server-auth exception.)
export async function logoutAction(): Promise<void> {
  await endSession()
  redirect("/login")
}

// Switch the active organization (persisted in the request-context cookie). Revalidate the whole
// dashboard so every org-scoped page re-renders against the new org without a hard navigation.
export async function selectActiveOrgAction(orgId: string): Promise<void> {
  await requireSession()
  await writeActiveOrgId(orgId)
  revalidatePath("/", "layout")
}
