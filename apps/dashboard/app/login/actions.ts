"use server";

import { redirect } from "next/navigation";

import { startSession, verifyPassword } from "@/lib/auth/session";

export type LoginState = { error: string | null };

// This is the auth boundary itself, so it is intentionally session-less: the password check below is
// its access control. (react-doctor's server-auth rule flags it as a known, accepted exception.)
export async function loginAction(_prev: LoginState, formData: FormData): Promise<LoginState> {
  const password = String(formData.get("password") ?? "");
  if (!verifyPassword(password)) {
    return { error: "Invalid password." };
  }
  await startSession();
  redirect("/");
}
