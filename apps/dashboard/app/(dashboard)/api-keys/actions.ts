"use server"

import { revalidatePath } from "next/cache"

import { requireSession } from "@/lib/auth/session"
import { createApiKey, revokeApiKey } from "@/lib/meter/client"
import type { ApiKeyRole, ApiKeyScope } from "@/lib/meter/types"

export type ActionResult = { ok: true } | { ok: false; error: string }
export type CreateResult =
  | { ok: true; token: string; prefix: string }
  | { ok: false; error: string }

export async function createApiKeyAction(input: {
  orgId: string
  name: string
  role: ApiKeyRole
  scope: ApiKeyScope
}): Promise<CreateResult> {
  try {
    await requireSession()
    const key = await createApiKey(input)
    revalidatePath("/api-keys")
    return { ok: true, token: key.token, prefix: key.prefix }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}

export async function revokeApiKeyAction(id: string): Promise<ActionResult> {
  try {
    await requireSession()
    await revokeApiKey(id)
    revalidatePath("/api-keys")
    return { ok: true }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}
