"use server";

import { revalidatePath } from "next/cache";

import { createOrganization } from "@/lib/meter/client";

export type ActionResult = { ok: true } | { ok: false; error: string };

export async function createOrganizationAction(input: {
  slug: string;
  name: string;
}): Promise<ActionResult> {
  try {
    await createOrganization(input);
    revalidatePath("/organizations");
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "request failed" };
  }
}
