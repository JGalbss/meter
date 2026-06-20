import { Buildings, PlugsConnected } from "@phosphor-icons/react/dist/ssr";

import { EmptyState } from "@/components/empty-state";
import { PageHeader } from "@/components/page-header";
import { Card, CardContent } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { listOrganizations } from "@/lib/meter/client";

export const dynamic = "force-dynamic";

export default async function OrganizationsPage() {
  const result = await listOrganizations();

  if (!result.ok) {
    return (
      <>
        <PageHeader title="Organizations" />
        <EmptyState icon={PlugsConnected} title="Control plane unreachable" message={result.error} />
      </>
    );
  }

  if (result.data.length === 0) {
    return (
      <>
        <PageHeader title="Organizations" description="Tenants in this meter deployment." />
        <EmptyState
          icon={Buildings}
          title="No organizations"
          message="Create one via the control plane API to get started."
        />
      </>
    );
  }

  return (
    <>
      <PageHeader title="Organizations" description="Tenants in this meter deployment." />
      <Card>
        <CardContent className="p-0">
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
                  <TableCell className="text-muted-foreground">{org.slug}</TableCell>
                  <TableCell>{org.defaultCurrency}</TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">{org.id}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </>
  );
}
