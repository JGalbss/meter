import { Key, PlugsConnected } from "@phosphor-icons/react/dist/ssr";

import { EmptyState } from "@/components/empty-state";
import { PageHeader } from "@/components/page-header";
import { ValueBadge } from "@/components/value-badge";
import { Card, CardContent } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { listApiKeys, unwrapOr } from "@/lib/meter/client";
import { resolveOrgScope } from "@/lib/meter/org";
import { CreateApiKeyDialog } from "./create-api-key-dialog";
import { RevokeApiKeyButton } from "./revoke-api-key-button";

export const dynamic = "force-dynamic";

const STATUS_VARIANTS = { active: "default", revoked: "outline" } as const;

function keyStatus(revokedAt: string | null): string {
  if (revokedAt === null) {
    return "active";
  }
  return "revoked";
}

function whenOrNever(at: string | null): string {
  if (at === null) {
    return "never";
  }
  return new Date(at).toLocaleString();
}

export default async function ApiKeysPage({
  searchParams,
}: {
  searchParams: Promise<{ org?: string }>;
}) {
  const { org } = await searchParams;
  const scope = await resolveOrgScope(org);

  if (scope.error !== null) {
    return (
      <>
        <PageHeader title="API keys" />
        <EmptyState icon={PlugsConnected} title="Control plane unreachable" message={scope.error} />
      </>
    );
  }

  if (scope.activeOrg === null) {
    return (
      <>
        <PageHeader title="API keys" />
        <EmptyState icon={Key} title="No organization" message="Create an organization first." />
      </>
    );
  }

  const orgId = scope.activeOrg.id;
  const keys = unwrapOr(await listApiKeys(orgId), []);

  return (
    <>
      <PageHeader
        title="API keys"
        description="Bearer tokens that authenticate control-plane requests."
        action={<CreateApiKeyDialog orgId={orgId} />}
      />
      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Prefix</TableHead>
                <TableHead>Created</TableHead>
                <TableHead>Last used</TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {keys.length === 0 && (
                <TableRow>
                  <TableCell colSpan={6} className="py-10 text-center text-sm text-muted-foreground">
                    No API keys.
                  </TableCell>
                </TableRow>
              )}
              {keys.map((key) => (
                <TableRow key={key.id}>
                  <TableCell className="font-medium">{key.name}</TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">
                    {key.prefix}…
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {new Date(key.createdAt).toLocaleDateString()}
                  </TableCell>
                  <TableCell className="text-muted-foreground">{whenOrNever(key.lastUsedAt)}</TableCell>
                  <TableCell>
                    <ValueBadge value={keyStatus(key.revokedAt)} variants={STATUS_VARIANTS} />
                  </TableCell>
                  <TableCell className="text-right">
                    {key.revokedAt === null && <RevokeApiKeyButton id={key.id} />}
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
