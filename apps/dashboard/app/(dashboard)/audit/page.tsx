import { ClipboardText } from "@phosphor-icons/react/dist/ssr"
import { Suspense } from "react"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { RevealOnLoad, TableSkeleton } from "@/components/section-skeleton"
import { Badge } from "@/components/ui/badge"
import type { BadgeVariant } from "@/components/value-badge"
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
import { fetchAuditLog } from "@/lib/meter/engine"
import type { AuditEntry } from "@/lib/meter/types"
import { AuditFilters } from "./audit-filters"
import { ExportAuditButton } from "./export-audit-button"

function statusVariant(status: number): BadgeVariant {
  if (status >= 400) {
    return "destructive"
  }
  return "default"
}

const WINDOW_MS: Record<string, number> = {
  "24h": 86_400_000,
  "7d": 604_800_000,
  "30d": 2_592_000_000,
}

// Translate a window preset into an RFC3339 lower bound the engine can filter on.
function sinceFor(window: string | undefined): string | undefined {
  if (window === undefined) {
    return undefined
  }
  const ms = WINDOW_MS[window]
  if (ms === undefined) {
    return undefined
  }
  return new Date(Date.now() - ms).toISOString()
}

export default async function AuditPage({
  searchParams,
}: {
  searchParams: Promise<{ actor?: string; method?: string; window?: string }>
}) {
  const { actor, method, window } = await searchParams

  return (
    <>
      <PageHeader
        title="Audit log"
        description="Every mutating request to the engine, most recent first."
        action={
          <Suspense fallback={<ExportAuditButton entries={[]} />}>
            <AuditExportAction actor={actor} method={method} window={window} />
          </Suspense>
        }
      />
      <AuditFilters actor={actor} method={method} window={window} />
      <Suspense fallback={<TableSkeleton />}>
        <AuditTable actor={actor} method={method} window={window} />
      </Suspense>
    </>
  )
}

// Audit entries for the current filters. The export button and the table both need the full result
// set; React memoizes identical fetches within a render, so each renders from one underlying request.
function fetchEntriesFor(
  actor: string | undefined,
  method: string | undefined,
  window: string | undefined
): Promise<readonly AuditEntry[]> {
  return fetchAuditLog({
    actor,
    method,
    since: sinceFor(window),
    limit: 500,
  }).then((result) => unwrapOr(result, []))
}

async function AuditExportAction({
  actor,
  method,
  window,
}: {
  actor: string | undefined
  method: string | undefined
  window: string | undefined
}) {
  const entries = await fetchEntriesFor(actor, method, window)
  return <ExportAuditButton entries={entries} />
}

async function AuditTable({
  actor,
  method,
  window,
}: {
  actor: string | undefined
  method: string | undefined
  window: string | undefined
}) {
  const entries = await fetchEntriesFor(actor, method, window)

  return (
    <RevealOnLoad>
      <Card className="py-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Time</TableHead>
              <TableHead>Actor</TableHead>
              <TableHead>Method</TableHead>
              <TableHead>Path</TableHead>
              <TableHead>Request</TableHead>
              <TableHead className="text-right">Status</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {entries.length === 0 && (
              <TableRow>
                <TableCell colSpan={6} className="p-0">
                  <EmptyState
                    icon={ClipboardText}
                    title="No audit entries"
                    message="No entries match these filters (or the engine is unreachable)."
                  />
                </TableCell>
              </TableRow>
            )}
            {entries.map((entry) => (
              <TableRow key={entry.id}>
                <TableCell className="text-muted-foreground">
                  {new Date(entry.created_at).toLocaleString()}
                </TableCell>
                <TableCell className="font-medium">{entry.actor}</TableCell>
                <TableCell className="font-mono text-xs">
                  {entry.method}
                </TableCell>
                <TableCell className="font-mono text-xs text-muted-foreground">
                  {entry.path}
                </TableCell>
                <TableCell className="font-mono text-xs text-muted-foreground">
                  {entry.request_id}
                </TableCell>
                <TableCell className="text-right">
                  <Badge variant={statusVariant(entry.status)}>
                    {entry.status}
                  </Badge>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Card>
    </RevealOnLoad>
  )
}
