import { Calculator } from "@phosphor-icons/react/dist/ssr"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { unwrapOr } from "@/lib/meter/client"
import { fetchCatalog } from "@/lib/meter/engine"
import { PricingSimulator } from "./pricing-simulator"

export const dynamic = "force-dynamic"

export default async function SimulatePage() {
  const catalog = unwrapOr(await fetchCatalog(), { as_of: "", models: [] })

  return (
    <>
      <PageHeader
        title="Pricing simulator"
        description="Compare what a usage profile costs across two catalogued models — a pure projection over the pricing layer that never touches the ledger."
      />
      {catalog.models.length === 0 ? (
        <EmptyState
          icon={Calculator}
          title="No rate cards"
          message="The engine's model catalog is empty or unreachable."
        />
      ) : (
        <PricingSimulator models={catalog.models} />
      )}
    </>
  )
}
