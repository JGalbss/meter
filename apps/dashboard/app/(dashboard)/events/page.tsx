import { ListBullets } from "@phosphor-icons/react/dist/ssr"
import { Suspense } from "react"

import { AccountSearchForm } from "@/components/account-search-form"
import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { RevealOnLoad, TableSkeleton } from "@/components/section-skeleton"
import { ValueBadge } from "@/components/value-badge"
import { Card } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { unwrapOr } from "@/lib/meter/client"
import { fetchEventsForAccount } from "@/lib/meter/engine"
import { resolveOrgScope } from "@/lib/meter/org"
import { AmendEventButton } from "./amend-event-button"
import { VoidRunButton } from "./void-run-button"

const STATUS_VARIANTS = {
  recorded: "default",
  amended: "secondary",
  voided: "outline",
} as const

function summarize(properties: unknown): string {
  if (properties === null || typeof properties !== "object") {
    return ""
  }
  return JSON.stringify(properties)
}

// A run can be voided from the UI only while it is the live (recorded) version and has a run id.
function canVoidRun(runId: string | null, status: string): boolean {
  return runId !== null && status === "recorded"
}

// Only the live version is amendable (amending a voided/superseded event is rejected by the engine).
function isRecorded(status: string): boolean {
  return status === "recorded"
}

function prettyProperties(properties: unknown): string {
  return JSON.stringify(properties ?? {}, null, 2)
}

export default async function EventsPage({
  searchParams,
}: {
  searchParams: Promise<{ account?: string }>
}) {
  const { account } = await searchParams
  await resolveOrgScope()
  const hasAccount = account !== undefined && account.length > 0

  return (
    <>
      <PageHeader
        title="Events"
        description="Usage events recorded against an engine account (latest version, non-voided)."
        action={
          <AccountSearchForm basePath="/events" initial={account ?? ""} />
        }
      />
      {!hasAccount && (
        <EmptyState
          icon={ListBullets}
          title="Choose an account"
          message="Enter an engine account id to browse its usage events."
        />
      )}
      {hasAccount && (
        <Suspense fallback={<TableSkeleton />}>
          <EventsData account={account} />
        </Suspense>
      )}
    </>
  )
}

async function EventsData({ account }: { account: string }) {
  const events = unwrapOr(await fetchEventsForAccount(account), [])

  return (
    <RevealOnLoad>
      <Card className="py-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Time</TableHead>
              <TableHead>Meter</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Run</TableHead>
              <TableHead>Properties</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {events.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={6}
                  className="py-10 text-center text-sm text-muted-foreground"
                >
                  No events for this account (or the engine is unreachable).
                </TableCell>
              </TableRow>
            )}
            {events.map((event) => (
              <TableRow key={event.id}>
                <TableCell className="text-muted-foreground">
                  {new Date(event.event_time).toLocaleString()}
                </TableCell>
                <TableCell className="font-medium">{event.meter}</TableCell>
                <TableCell>
                  <ValueBadge value={event.status} variants={STATUS_VARIANTS} />
                </TableCell>
                <TableCell className="font-mono text-xs text-muted-foreground">
                  {event.run_id ?? "—"}
                </TableCell>
                <TableCell className="max-w-md truncate font-mono text-xs text-muted-foreground">
                  {summarize(event.properties)}
                </TableCell>
                <TableCell className="text-right">
                  <div className="flex justify-end gap-2">
                    {isRecorded(event.status) && (
                      <AmendEventButton
                        eventId={event.id}
                        properties={prettyProperties(event.properties)}
                      />
                    )}
                    {canVoidRun(event.run_id, event.status) &&
                      event.run_id !== null && (
                        <VoidRunButton runId={event.run_id} />
                      )}
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Card>
    </RevealOnLoad>
  )
}
