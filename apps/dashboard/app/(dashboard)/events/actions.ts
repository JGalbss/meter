"use server"

import { revalidatePath } from "next/cache"

import { requireSession } from "@/lib/auth/session"
import { amendEvent, voidRun } from "@/lib/meter/engine"

export type VoidRunResult =
  | { ok: true; voided: number }
  | { ok: false; error: string }

export type AmendResult = { ok: true } | { ok: false; error: string }

function parseJson(raw: string): { ok: true; value: unknown } | { ok: false } {
  try {
    return { ok: true, value: JSON.parse(raw) }
  } catch {
    return { ok: false }
  }
}

export async function amendEventAction(
  eventId: string,
  propertiesJson: string
): Promise<AmendResult> {
  try {
    await requireSession()
    const parsed = parseJson(propertiesJson)
    if (!parsed.ok) {
      return { ok: false, error: "Properties must be valid JSON." }
    }
    const result = await amendEvent(eventId, parsed.value)
    if (!result.ok) {
      return { ok: false, error: result.error }
    }
    revalidatePath("/events")
    return { ok: true }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}

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
