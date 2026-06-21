import { Receipt } from "@phosphor-icons/react/dist/ssr"
import { Suspense } from "react"

import { AccountSearchForm } from "@/components/account-search-form"
import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { RevealOnLoad, StatsSkeleton } from "@/components/section-skeleton"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { unwrapOr } from "@/lib/meter/client"
import { fetchInvoice, fetchUsageByDay } from "@/lib/meter/engine"

function creditDisplay(value: string): string {
  const parsed = Number(value)
  if (Number.isNaN(parsed)) {
    return value
  }
  return parsed.toLocaleString()
}

// The billing period is month-to-date in UTC — the deterministic window the engine sums the ledger over.
function monthToDate(): { start: string; end: string; label: string } {
  const now = new Date()
  const start = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1))
  const fmt = (d: Date) =>
    d.toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
      timeZone: "UTC",
    })
  return {
    start: start.toISOString(),
    end: now.toISOString(),
    label: `${fmt(start)} – ${fmt(now)}`,
  }
}

export default async function InvoicesPage({
  searchParams,
}: {
  searchParams: Promise<{ account?: string }>
}) {
  const { account } = await searchParams
  const hasAccount = account !== undefined && account.length > 0

  return (
    <>
      <PageHeader
        title="Invoices"
        description="A deterministic statement summed from the ledger (enforced equals billed)."
        action={
          <AccountSearchForm basePath="/invoices" initial={account ?? ""} />
        }
      />

      {!hasAccount && (
        <EmptyState
          icon={Receipt}
          title="Choose an account"
          message="Enter an engine account id to generate its current statement."
        />
      )}

      {hasAccount && (
        <Suspense fallback={<StatsSkeleton />}>
          <InvoiceData account={account} />
        </Suspense>
      )}
    </>
  )
}

async function InvoiceData({ account }: { account: string }) {
  const period = monthToDate()
  const invoiceResult = await fetchInvoice(account, period.start, period.end)
  const days = unwrapOr(
    await fetchUsageByDay(account, period.start, period.end),
    []
  )

  if (!invoiceResult.ok) {
    return (
      <RevealOnLoad>
        <EmptyState
          icon={Receipt}
          title="Statement unavailable"
          message="That account was not found, or the engine is unreachable."
        />
      </RevealOnLoad>
    )
  }

  return (
    <RevealOnLoad>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span>Statement</span>
            <span className="text-sm font-normal text-muted-foreground">
              {period.label}
            </span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid gap-4 sm:grid-cols-2">
            <div>
              <p className="text-sm text-muted-foreground">Total credits</p>
              <p className="font-heading text-4xl font-semibold tabular-nums">
                {creditDisplay(invoiceResult.data.total_credits)}
              </p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Ledger entries</p>
              <p className="font-heading text-4xl font-semibold tabular-nums">
                {invoiceResult.data.entries.toLocaleString()}
              </p>
            </div>
          </div>
          <p className="mt-4 font-mono text-xs text-muted-foreground">
            {invoiceResult.data.account_id}
          </p>
        </CardContent>
      </Card>

      <Card className="mt-6">
        <CardHeader>
          <CardTitle>Daily breakdown</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Day</TableHead>
                <TableHead className="text-right">Entries</TableHead>
                <TableHead className="text-right">Credits</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {days.length === 0 && (
                <TableRow>
                  <TableCell
                    colSpan={3}
                    className="py-10 text-center text-sm text-muted-foreground"
                  >
                    No usage in this period.
                  </TableCell>
                </TableRow>
              )}
              {days.map((day) => (
                <TableRow key={day.day}>
                  <TableCell>{day.day}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {day.entry_count}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {creditDisplay(day.total_credits)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </RevealOnLoad>
  )
}
