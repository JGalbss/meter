"use server";

import { revalidatePath } from "next/cache";

import { setWebhookEnabled } from "@/lib/meter/client";

export type ActionResult = { ok: true } | { ok: false; error: string };

export async function toggleWebhookAction(id: string, enabled: boolean): Promise<ActionResult> {
  try {
    await setWebhookEnabled(id, enabled);
    revalidatePath("/webhooks");
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "request failed" };
  }
}
