import { Wallet } from "@phosphor-icons/react/dist/ssr"
import { Suspense } from "react"

import { AccountSearchForm } from "@/components/account-search-form"
import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { RevealOnLoad, StatsSkeleton } from "@/components/section-skeleton"
import { ValueBadge } from "@/components/value-badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { fetchBalance, fetchEntries } from "@/lib/meter/engine"
import { resolveOrgScope } from "@/lib/meter/org"
import type { Balance } from "@/lib/meter/types"

const ENTRY_VARIANTS = {
  grant: "default",
  refund: "default",
  usage: "secondary",
  settle: "secondary",
  transfer: "secondary",
  chargeback: "destructive",
} as const

function creditDisplay(value: string): string {
  const parsed = Number(value)
  if (Number.isNaN(parsed)) {
    return value
  }
  return parsed.toLocaleString()
}

function available(balance: Balance): string {
  const settled = Number(balance.settled)
  const held = Number(balance.held)
  if (Number.isNaN(settled) || Number.isNaN(held)) {
    return balance.settled
  }
  return (settled - held).toLocaleString()
}

export default async function AccountsPage({
  searchParams,
}: {
  searchParams: Promise<{ account?: string }>
}) {
  const { account } = await searchParams
  const scope = await resolveOrgScope()
  const orgId = scope.activeOrg?.id
  const hasAccount = account !== undefined && account.length > 0

  return (
    <>
      <PageHeader
        title="Accounts"
        description="An engine account's balance and immutable ledger history."
        action={
          <AccountSearchForm
            basePath="/accounts"
            initial={account ?? ""}
            org={orgId}
          />
        }
      />

      {!hasAccount && (
        <EmptyState
          icon={Wallet}
          title="Choose an account"
          message="Enter an engine account id to see its balance and ledger entries."
        />
      )}

      {hasAccount && (
        <Suspense fallback={<StatsSkeleton />}>
          <AccountLedger account={account} />
        </Suspense>
      )}
    </>
  )
}

async function AccountLedger({ account }: { account: string }) {
  const balanceResult = await fetchBalance(account)
  const entriesResult = await fetchEntries(account)
  const ledgerEntries = entriesResult.ok ? entriesResult.data : []

  if (!balanceResult.ok) {
    return (
      <EmptyState
        icon={Wallet}
        title="Account unavailable"
        message="That account was not found, or the engine is unreachable."
      />
    )
  }

  return (
    <RevealOnLoad>
      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Available
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="font-heading text-3xl font-semibold tabular-nums">
              {available(balanceResult.data)}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Settled
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="font-heading text-3xl font-semibold tabular-nums">
              {creditDisplay(balanceResult.data.settled)}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Held
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="font-heading text-3xl font-semibold tabular-nums">
              {creditDisplay(balanceResult.data.held)}
            </p>
          </CardContent>
        </Card>
      </div>

      <Card className="mt-6">
        <CardHeader>
          <CardTitle>Ledger entries</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Time</TableHead>
                <TableHead>Type</TableHead>
                <TableHead className="text-right">Delta</TableHead>
                <TableHead className="text-right">Balance after</TableHead>
                <TableHead>Source</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {ledgerEntries.length === 0 && (
                <TableRow>
                  <TableCell
                    colSpan={5}
                    className="py-10 text-center text-sm text-muted-foreground"
                  >
                    No ledger entries for this account.
                  </TableCell>
                </TableRow>
              )}
              {ledgerEntries.map((entry) => (
                <TableRow key={entry.id}>
                  <TableCell className="text-muted-foreground">
                    {new Date(entry.created_at).toLocaleString()}
                  </TableCell>
                  <TableCell>
                    <ValueBadge
                      value={entry.entry_type}
                      variants={ENTRY_VARIANTS}
                    />
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {creditDisplay(entry.delta_credits)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {creditDisplay(entry.balance_after)}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {entry.source ?? "—"}
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
