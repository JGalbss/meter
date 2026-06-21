import { Buildings, PlugsConnected } from "@phosphor-icons/react/dist/ssr"
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
import { listOrganizations } from "@/lib/meter/client"
import { CreateOrganizationDialog } from "./create-organization-dialog"

export default async function OrganizationsPage() {
  return (
    <>
      <PageHeader
        title="Organizations"
        description="Tenants in this meter deployment."
        action={<CreateOrganizationDialog />}
      />
      <Suspense fallback={<TableSkeleton />}>
        <OrganizationsTable />
      </Suspense>
    </>
  )
}

async function OrganizationsTable() {
  const result = await listOrganizations()

  if (!result.ok) {
    return (
      <EmptyState
        icon={PlugsConnected}
        title="Control plane unreachable"
        message={result.error}
      />
    )
  }

  if (result.data.length === 0) {
    return (
      <EmptyState
        icon={Buildings}
        title="No organizations"
        message="Create one via the control plane API to get started."
      />
    )
  }

  return (
    <RevealOnLoad>
      <Card className="py-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Slug</TableHead>
              <TableHead>Currency</TableHead>
              <TableHead>ID</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {result.data.map((org) => (
              <TableRow key={org.id}>
                <TableCell className="font-medium">{org.name}</TableCell>
                <TableCell className="text-muted-foreground">
                  {org.slug}
                </TableCell>
                <TableCell>{org.defaultCurrency}</TableCell>
                <TableCell className="font-mono text-xs text-muted-foreground">
                  {org.id}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Card>
    </RevealOnLoad>
  )
}
