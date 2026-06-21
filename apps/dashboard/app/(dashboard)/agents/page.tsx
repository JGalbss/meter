import { Package, PlugsConnected } from "@phosphor-icons/react/dist/ssr"
import { Suspense } from "react"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { RevealOnLoad, TableSkeleton } from "@/components/section-skeleton"
import { Card } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { listAgents, unwrapOr } from "@/lib/meter/client"
import { resolveOrgScope } from "@/lib/meter/org"
import { CreateAgentDialog } from "./create-agent-dialog"

export default async function AgentsPage() {
  const scope = await resolveOrgScope()

  if (scope.error !== null) {
    return (
      <>
        <PageHeader title="Agents" />
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
        <PageHeader title="Agents" />
        <EmptyState
          icon={Package}
          title="No organization"
          message="Create an organization first."
        />
      </>
    )
  }

  const orgId = scope.activeOrg.id

  return (
    <>
      <PageHeader
        title="Agents"
        description="Metered agents in this organization."
        action={<CreateAgentDialog orgId={orgId} />}
      />
      <Suspense fallback={<TableSkeleton />}>
        <AgentsTable orgId={orgId} />
      </Suspense>
    </>
  )
}

async function AgentsTable({ orgId }: { orgId: string }) {
  const agents = unwrapOr(await listAgents(orgId), [])

  return (
    <RevealOnLoad>
      <Card className="py-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Key</TableHead>
              <TableHead>ID</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {agents.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={3}
                  className="py-10 text-center text-sm text-muted-foreground"
                >
                  No agents.
                </TableCell>
              </TableRow>
            )}
            {agents.map((agent) => (
              <TableRow key={agent.id}>
                <TableCell className="font-medium">{agent.name}</TableCell>
                <TableCell className="font-mono text-xs text-muted-foreground">
                  {agent.key}
                </TableCell>
                <TableCell className="font-mono text-xs text-muted-foreground">
                  {agent.id}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Card>
    </RevealOnLoad>
  )
}
