"use server"

import { requireSession } from "@/lib/auth/session"
import { fetchSimulate } from "@/lib/meter/engine"
import type { SimulateInput, SimulateResult } from "@/lib/meter/types"

export type SimulateActionResult =
  | { ok: true; summary: SimulateResult }
  | { ok: false; error: string }

// Simulation never moves money, but it reads the engine, so gate it behind a session like every
// other server action.
export async function simulateAction(
  input: SimulateInput
): Promise<SimulateActionResult> {
  try {
    await requireSession()
    const result = await fetchSimulate(input)
    if (!result.ok) {
      return { ok: false, error: result.error }
    }
    return { ok: true, summary: result.data }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}
