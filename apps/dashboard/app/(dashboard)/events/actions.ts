"use server"

import { revalidatePath } from "next/cache"

import { requireSession } from "@/lib/auth/session"
import { voidRun } from "@/lib/meter/engine"

export type VoidRunResult =
  | { ok: true; voided: number }
  | { ok: false; error: string }

export async function voidRunAction(runId: string): Promise<VoidRunResult> {
  try {
    await requireSession()
    const result = await voidRun(runId)
    if (!result.ok) {
      return { ok: false, error: result.error }
    }
    revalidatePath("/events")
    return { ok: true, voided: result.data }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}
