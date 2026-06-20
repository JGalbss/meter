"use server";

import { revalidatePath } from "next/cache";

import { setAlertRuleEnabled } from "@/lib/meter/client";

export type ActionResult = { ok: true } | { ok: false; error: string };

export async function toggleAlertRuleAction(id: string, enabled: boolean): Promise<ActionResult> {
  try {
    await setAlertRuleEnabled(id, enabled);
    revalidatePath("/alerts");
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "request failed" };
  }
}
