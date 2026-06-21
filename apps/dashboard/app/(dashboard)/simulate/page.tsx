import { Calculator } from "@phosphor-icons/react/dist/ssr"
import { Suspense } from "react"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { RevealOnLoad, TableSkeleton } from "@/components/section-skeleton"
import { unwrapOr } from "@/lib/meter/client"
import { fetchCatalog } from "@/lib/meter/engine"
import { PricingSimulator } from "./pricing-simulator"

export default async function SimulatePage() {
  return (
    <>
      <PageHeader
        title="Pricing simulator"
        description="Compare what a usage profile costs across two catalogued models — a pure projection over the pricing layer that never touches the ledger."
      />
      <Suspense fallback={<TableSkeleton />}>
        <Simulator />
      </Suspense>
    </>
  )
}

async function Simulator() {
  const catalog = unwrapOr(await fetchCatalog(), { as_of: "", models: [] })

  if (catalog.models.length === 0) {
    return (
      <RevealOnLoad>
        <EmptyState
          icon={Calculator}
          title="No rate cards"
          message="The engine's model catalog is empty or unreachable."
        />
      </RevealOnLoad>
    )
  }

  return (
    <RevealOnLoad>
      <PricingSimulator models={catalog.models} />
    </RevealOnLoad>
  )
}
