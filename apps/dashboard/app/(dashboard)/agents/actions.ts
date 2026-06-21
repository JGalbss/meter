"use server"

import { revalidatePath } from "next/cache"

import { requireSession } from "@/lib/auth/session"
import { createAgent } from "@/lib/meter/client"

export type ActionResult = { ok: true } | { ok: false; error: string }

export async function createAgentAction(input: {
  orgId: string
  key: string
  name: string
}): Promise<ActionResult> {
  try {
    await requireSession()
    await createAgent(input)
    revalidatePath("/agents")
    return { ok: true }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}
