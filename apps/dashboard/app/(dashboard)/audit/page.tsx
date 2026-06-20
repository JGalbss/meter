import { ClipboardText } from "@phosphor-icons/react/dist/ssr"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { Badge } from "@/components/ui/badge"
import type { BadgeVariant } from "@/components/value-badge"
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
import { fetchAuditLog } from "@/lib/meter/engine"

export const dynamic = "force-dynamic"

function statusVariant(status: number): BadgeVariant {
  if (status >= 400) {
    return "destructive"
  }
  return "default"
}

export default async function AuditPage() {
  const entries = unwrapOr(await fetchAuditLog(200), [])

  return (
    <>
      <PageHeader
        title="Audit log"
        description="Every mutating request to the engine, most recent first."
      />
      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Time</TableHead>
                <TableHead>Actor</TableHead>
                <TableHead>Method</TableHead>
                <TableHead>Path</TableHead>
                <TableHead className="text-right">Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {entries.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5} className="p-0">
                    <EmptyState
                      icon={ClipboardText}
                      title="No audit entries"
                      message="Mutating engine requests will appear here (or the engine is unreachable)."
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
                  <TableCell className="text-right">
                    <Badge variant={statusVariant(entry.status)}>
                      {entry.status}
                    </Badge>
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
