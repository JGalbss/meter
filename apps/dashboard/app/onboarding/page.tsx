import { redirect } from "next/navigation"

import { OnboardingWizard } from "@/components/onboarding/onboarding-wizard"
import { hasValidSession } from "@/lib/auth/session"
import { resolveOrgScope } from "@/lib/meter/org"
import type { Organization } from "@/lib/meter/types"

function toInitialOrg(
  activeOrg: Organization | null
): { id: string; name: string } | null {
  if (activeOrg === null) {
    return null
  }
  return { id: activeOrg.id, name: activeOrg.name }
}

export default async function OnboardingPage() {
  if (!(await hasValidSession())) {
    redirect("/login")
  }

  const scope = await resolveOrgScope()

  return (
    <main className="flex min-h-svh items-center justify-center p-6">
      <OnboardingWizard initialOrg={toInitialOrg(scope.activeOrg)} />
    </main>
  )
}
