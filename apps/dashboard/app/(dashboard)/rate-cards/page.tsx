import { Tag } from "@phosphor-icons/react/dist/ssr"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { Card, CardContent } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { unwrapOr } from "@/lib/meter/client"
import { fetchCatalog } from "@/lib/meter/engine"
import { pricePerMillionTokens } from "@/lib/meter/pricing-format"

export const dynamic = "force-dynamic"

export default async function RateCardsPage() {
  const catalog = unwrapOr(await fetchCatalog(), { as_of: "", models: [] })
  const asOf = catalog.as_of.length > 0 ? ` Prices as of ${catalog.as_of}.` : ""

  return (
    <>
      <PageHeader
        title="Rate cards"
        description={`The hosted model catalog the engine prices against — provider cost per 1M tokens.${asOf} Best-effort; verify against the provider before billing.`}
      />
      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Provider</TableHead>
                <TableHead>Model</TableHead>
                <TableHead className="text-right">Input</TableHead>
                <TableHead className="text-right">Cache read</TableHead>
                <TableHead className="text-right">Cache write</TableHead>
                <TableHead className="text-right">Output</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {catalog.models.length === 0 && (
                <TableRow>
                  <TableCell colSpan={6} className="p-0">
                    <EmptyState
                      icon={Tag}
                      title="No rate cards"
                      message="The engine's model catalog will appear here (or the engine is unreachable)."
                    />
                  </TableCell>
                </TableRow>
              )}
              {catalog.models.map((entry) => (
                <TableRow key={`${entry.provider}:${entry.model_id}`}>
                  <TableCell className="text-muted-foreground">
                    {entry.provider}
                  </TableCell>
                  <TableCell className="font-mono text-xs font-medium">
                    {entry.model_id}
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs">
                    {pricePerMillionTokens(entry.input_per_token)}
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs">
                    {pricePerMillionTokens(entry.cache_read_per_token)}
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs">
                    {pricePerMillionTokens(entry.cache_write_per_token)}
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs">
                    {pricePerMillionTokens(entry.output_per_token)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </>
  )
}
