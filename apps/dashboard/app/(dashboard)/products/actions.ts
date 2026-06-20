"use server";

import { revalidatePath } from "next/cache";

import { createProduct } from "@/lib/meter/client";

export type ActionResult = { ok: true } | { ok: false; error: string };

export async function createProductAction(input: {
  orgId: string;
  key: string;
  name: string;
}): Promise<ActionResult> {
  try {
    await createProduct(input);
    revalidatePath("/products");
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : "request failed" };
  }
}
