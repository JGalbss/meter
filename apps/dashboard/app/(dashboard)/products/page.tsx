import { Package, PlugsConnected } from "@phosphor-icons/react/dist/ssr";

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
import { listProducts, unwrapOr } from "@/lib/meter/client";
import { resolveOrgScope } from "@/lib/meter/org";
import { CreateProductDialog } from "./create-product-dialog";

export const dynamic = "force-dynamic";

export default async function ProductsPage({
  searchParams,
}: {
  searchParams: Promise<{ org?: string }>;
}) {
  const { org } = await searchParams;
  const scope = await resolveOrgScope(org);

  if (scope.error !== null) {
    return (
      <>
        <PageHeader title="Products" />
        <EmptyState icon={PlugsConnected} title="Control plane unreachable" message={scope.error} />
      </>
    );
  }

  if (scope.activeOrg === null) {
    return (
      <>
        <PageHeader title="Products" />
        <EmptyState icon={Package} title="No organization" message="Create an organization first." />
      </>
    );
  }

  const orgId = scope.activeOrg.id;
  const products = unwrapOr(await listProducts(orgId), []);

  return (
    <>
      <PageHeader
        title="Products"
        description="Metered products and agents in this organization."
        action={<CreateProductDialog orgId={orgId} />}
      />
      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Key</TableHead>
                <TableHead>ID</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {products.length === 0 && (
                <TableRow>
                  <TableCell colSpan={3} className="py-10 text-center text-sm text-muted-foreground">
                    No products.
                  </TableCell>
                </TableRow>
              )}
              {products.map((product) => (
                <TableRow key={product.id}>
                  <TableCell className="font-medium">{product.name}</TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">
                    {product.key}
                  </TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">
                    {product.id}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </>
  );
}
