"use server";

import { redirect } from "next/navigation";

import { endSession } from "@/lib/auth/session";

// Intentionally session-less: clearing your own session is safe for anyone to call, and gating logout
// behind a valid session would strand expired sessions. (Accepted react-doctor server-auth exception.)
export async function logoutAction(): Promise<void> {
  await endSession();
  redirect("/login");
}
