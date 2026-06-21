"use server"

import { randomUUID } from "node:crypto"

import { redirect } from "next/navigation"

import { requireSession } from "@/lib/auth/session"
import { writeActiveOrgId } from "@/lib/meter/active-org"
import {
  createAgent,
  createApiKey,
  createOrganization,
  listOrganizations,
} from "@/lib/meter/client"
import {
  fetchBalance,
  grantCredits,
  meterUsage,
  openAccount,
} from "@/lib/meter/engine"
import { dismissOnboarding } from "@/lib/onboarding"

export type CreateOrgResult =
  | { ok: true; orgId: string; orgName: string }
  | { ok: false; error: string }

export type CreateAgentResult = { ok: true } | { ok: false; error: string }

export type CreateKeyResult =
  | { ok: true; token: string; prefix: string }
  | { ok: false; error: string }

export interface TestPing {
  readonly accountId: string
  readonly model: string
  readonly granted: string
  readonly credits: string
  readonly balanceBefore: string
  readonly balanceAfter: string
  readonly eventId: string
}

export type TestPingResult =
  | { ok: true; ping: TestPing }
  | { ok: false; error: string }

// The grant the test ping funds the demo account with, so the metered event has credits to burn.
const TEST_GRANT_CREDITS = "1000"

function slugify(value: string): string {
  const base = value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
  if (base.length === 0) {
    return "workspace"
  }
  return base
}

function settledOf(balance: { settled: string }): string {
  return balance.settled
}

// Create the org, make it the active one, and return its id so the wizard can keep going without a
// page round-trip. We re-list to resolve the new id (the create endpoint returns no body).
export async function createOrgAction(input: {
  name: string
}): Promise<CreateOrgResult> {
  try {
    await requireSession()
    const slug = slugify(input.name)
    await createOrganization({ slug, name: input.name })
    const list = await listOrganizations()
    if (!list.ok) {
      return { ok: false, error: list.error }
    }
    const created = list.data.find((org) => org.slug === slug)
    if (created === undefined) {
      return { ok: false, error: "created organization not found" }
    }
    await writeActiveOrgId(created.id)
    return { ok: true, orgId: created.id, orgName: created.name }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}

export async function createAgentAction(input: {
  orgId: string
  name: string
}): Promise<CreateAgentResult> {
  try {
    await requireSession()
    await createAgent({
      orgId: input.orgId,
      key: slugify(input.name),
      name: input.name,
    })
    return { ok: true }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}

export async function createKeyAction(input: {
  orgId: string
  name: string
}): Promise<CreateKeyResult> {
  try {
    await requireSession()
    const key = await createApiKey({
      orgId: input.orgId,
      name: input.name,
      role: "member",
      scope: "org",
    })
    return { ok: true, token: key.token, prefix: key.prefix }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}

// The magical payoff: open a demo account, fund it, meter one real event, and report the balance
// before/after so the wizard can watch the credit tick down. All money math happens in the engine.
export async function runTestPingAction(input: {
  orgId: string
  model: string
}): Promise<TestPingResult> {
  try {
    await requireSession()
    const opened = await openAccount({ orgId: input.orgId, scope: "org" })
    if (!opened.ok) {
      return { ok: false, error: opened.error }
    }
    const accountId = opened.data.id

    const granted = await grantCredits(accountId, {
      amount: TEST_GRANT_CREDITS,
      source: "grant",
      idempotencyKey: randomUUID(),
    })
    if (!granted.ok) {
      return { ok: false, error: granted.error }
    }

    const before = await fetchBalance(accountId)
    if (!before.ok) {
      return { ok: false, error: before.error }
    }

    const metered = await meterUsage({
      orgId: input.orgId,
      account: accountId,
      model: input.model,
      idempotencyKey: randomUUID(),
      usage: { input_uncached: 1200, output: 350 },
    })
    if (!metered.ok) {
      return { ok: false, error: metered.error }
    }

    return {
      ok: true,
      ping: {
        accountId,
        model: input.model,
        granted: TEST_GRANT_CREDITS,
        credits: metered.data.credits,
        balanceBefore: settledOf(before.data),
        balanceAfter: metered.data.settled,
        eventId: metered.data.event_id,
      },
    }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "request failed",
    }
  }
}

export async function finishOnboardingAction(): Promise<void> {
  await requireSession()
  await dismissOnboarding()
  redirect("/")
}
