import {
  ChartBar,
  ChartLineUp,
  PlugsConnected,
} from "@phosphor-icons/react/dist/ssr"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { AccountSearchForm } from "@/components/account-search-form"
import { unwrapOr } from "@/lib/meter/client"
import { fetchUsageByDay, fetchUsageByModel } from "@/lib/meter/engine"
import { resolveOrgScope } from "@/lib/meter/org"
import { ModelUsageChartLazy } from "./model-usage-chart-lazy"
import { UsageChartLazy } from "./usage-chart-lazy"

export const dynamic = "force-dynamic"

const DAY_MS = 86_400_000

function thirtyDayWindow(): { start: string; end: string } {
  const now = new Date()
  const start = new Date(now.getTime() - 30 * DAY_MS)
  return { start: start.toISOString(), end: now.toISOString() }
}

export default async function UsagePage({
  searchParams,
}: {
  searchParams: Promise<{ org?: string; account?: string }>
}) {
  const { org, account } = await searchParams
  const scope = await resolveOrgScope(org)

  if (scope.error !== null) {
    return (
      <>
        <PageHeader title="Usage" />
        <EmptyState
          icon={PlugsConnected}
          title="Control plane unreachable"
          message={scope.error}
        />
      </>
    )
  }

  if (scope.activeOrg === null) {
    return (
      <>
        <PageHeader
          title="Usage"
          description="Usage analytics across your organization."
        />
        <EmptyState
          icon={ChartBar}
          title="No organization"
          message="Create an organization in the control plane to see usage."
        />
      </>
    )
  }

  const orgId = scope.activeOrg.id
  const byModel = unwrapOr(await fetchUsageByModel(orgId), [])
  const hasAccount = account !== undefined && account.length > 0
  const { start, end } = thirtyDayWindow()
  const series = hasAccount
    ? unwrapOr(await fetchUsageByDay(account, start, end), [])
    : []

  return (
    <>
      <PageHeader
        title="Usage"
        description={`Usage analytics for ${scope.activeOrg.name}.`}
      />

      <Card>
        <CardHeader>
          <CardTitle>Usage by model</CardTitle>
          <CardDescription>
            Events, tokens, and credits per model — reflects amends and voids.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          {byModel.length === 0 && (
            <p className="py-10 text-center text-sm text-muted-foreground">
              No usage recorded yet (or the engine is unreachable).
            </p>
          )}
          {byModel.length > 0 && (
            <>
              <ModelUsageChartLazy data={byModel} />
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Model</TableHead>
                    <TableHead className="text-right">Events</TableHead>
                    <TableHead className="text-right">Input tokens</TableHead>
                    <TableHead className="text-right">Output tokens</TableHead>
                    <TableHead className="text-right">Credits</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {byModel.map((usage) => (
                    <TableRow key={usage.model}>
                      <TableCell className="font-mono text-xs">
                        {usage.model}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {usage.events.toLocaleString()}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {usage.input_tokens.toLocaleString()}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {usage.output_tokens.toLocaleString()}
                      </TableCell>
                      <TableCell className="text-right font-medium tabular-nums">
                        {usage.credits.toLocaleString()}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </>
          )}
        </CardContent>
      </Card>

      <Card className="mt-6">
        <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
          <div className="space-y-1.5">
            <CardTitle>Daily credit burn</CardTitle>
            <CardDescription>
              Credit usage over the last 30 days for a single engine account.
            </CardDescription>
          </div>
          <AccountSearchForm
            basePath="/usage"
            initial={account ?? ""}
            org={orgId}
          />
        </CardHeader>
        <CardContent>
          {!hasAccount && (
            <EmptyState
              icon={ChartLineUp}
              title="Choose an account"
              message="Enter an engine account id to chart its daily credit usage."
            />
          )}
          {hasAccount && series.length === 0 && (
            <p className="py-10 text-center text-sm text-muted-foreground">
              No usage in this window (or the engine is unreachable).
            </p>
          )}
          {hasAccount && series.length > 0 && <UsageChartLazy data={series} />}
        </CardContent>
      </Card>
    </>
  )
}
