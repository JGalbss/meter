"use server";

import { revalidatePath } from "next/cache";

import { evaluateAlertRules, setAlertRuleEnabled } from "@/lib/meter/client";

export type ActionResult = { ok: true } | { ok: false; error: string };
export type EvaluateResult =
  | { ok: true; evaluated: number; raised: number }
  | { ok: false; error: string };

export async function toggleAlertRuleAction(id: string, enabled: boolean): Promise<ActionResult> {
  try {
    await setAlertRuleEnabled(id, enabled);
    revalidatePath("/alerts");
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "request failed" };
  }
}

export async function evaluateAction(orgId: string): Promise<EvaluateResult> {
  try {
    const summary = await evaluateAlertRules(orgId);
    revalidatePath("/alerts");
    return { ok: true, evaluated: summary.evaluated, raised: summary.raised };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "request failed" };
  }
}
