"use server";

import { revalidatePath } from "next/cache";

import { requireSession } from "@/lib/auth/session";
import { createWebhook, setWebhookEnabled } from "@/lib/meter/client";

export type ActionResult = { ok: true } | { ok: false; error: string };

export async function toggleWebhookAction(id: string, enabled: boolean): Promise<ActionResult> {
  try {
    await requireSession();
    await setWebhookEnabled(id, enabled);
    revalidatePath("/webhooks");
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "request failed" };
  }
}

export async function createWebhookAction(input: {
  orgId: string;
  url: string;
  secret: string;
  eventTypes: readonly string[];
}): Promise<ActionResult> {
  try {
    await requireSession();
    await createWebhook(input);
    revalidatePath("/webhooks");
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "request failed" };
  }
}
