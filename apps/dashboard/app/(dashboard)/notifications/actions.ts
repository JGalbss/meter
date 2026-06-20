"use server";

import { revalidatePath } from "next/cache";

import { ackNotification, markNotificationRead } from "@/lib/meter/client";

export type ActionResult = { ok: true } | { ok: false; error: string };

function fail(error: unknown): ActionResult {
  return { ok: false, error: error instanceof Error ? error.message : "request failed" };
}

export async function markReadAction(id: string): Promise<ActionResult> {
  try {
    await markNotificationRead(id);
    revalidatePath("/notifications");
    return { ok: true };
  } catch (error) {
    return fail(error);
  }
}

export async function ackAction(id: string): Promise<ActionResult> {
  try {
    await ackNotification(id);
    revalidatePath("/notifications");
    return { ok: true };
  } catch (error) {
    return fail(error);
  }
}
