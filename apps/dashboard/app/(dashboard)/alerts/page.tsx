import { PlugsConnected, ShieldWarning } from "@phosphor-icons/react/dist/ssr";

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
import { listAlertRules, unwrapOr } from "@/lib/meter/client";
import { resolveOrgScope } from "@/lib/meter/org";
import { AlertToggle } from "./alert-toggle";
import { CreateAlertRuleDialog } from "./create-alert-rule-dialog";
import { EvaluateButton } from "./evaluate-button";

export const dynamic = "force-dynamic";

const ENABLED_VARIANTS = { enabled: "default", disabled: "outline" } as const;
const STATUS_VARIANTS = { ok: "secondary", warning: "default", exceeded: "destructive" } as const;

function enabledLabel(enabled: boolean): string {
  if (enabled) {
    return "enabled";
  }
  return "disabled";
}

function statusLabel(status: string | null): string {
  if (status === null) {
    return "—";
  }
  return status;
}

export default async function AlertsPage({
  searchParams,
}: {
  searchParams: Promise<{ org?: string }>;
}) {
  const { org } = await searchParams;
  const scope = await resolveOrgScope(org);

  if (scope.error !== null) {
    return (
      <>
        <PageHeader title="Alert rules" />
        <EmptyState icon={PlugsConnected} title="Control plane unreachable" message={scope.error} />
      </>
    );
  }

  if (scope.activeOrg === null) {
    return (
      <>
        <PageHeader title="Alert rules" />
        <EmptyState
          icon={ShieldWarning}
          title="No organization"
          message="Create an organization first."
        />
      </>
    );
  }

  const orgId = scope.activeOrg.id;
  const rules = unwrapOr(await listAlertRules(orgId), []);

  return (
    <>
      <PageHeader
        title="Alert rules"
        description="Thresholds that raise notifications and fire webhooks."
        action={
          <div className="flex gap-2">
            <CreateAlertRuleDialog orgId={orgId} />
            <EvaluateButton orgId={orgId} />
          </div>
        }
      />
      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Scope</TableHead>
                <TableHead>Metric</TableHead>
                <TableHead>Threshold</TableHead>
                <TableHead>Action</TableHead>
                <TableHead>Last status</TableHead>
                <TableHead>Enabled</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rules.length === 0 && (
                <TableRow>
                  <TableCell colSpan={8} className="py-10 text-center text-sm text-muted-foreground">
                    No alert rules.
                  </TableCell>
                </TableRow>
              )}
              {rules.map((rule) => (
                <TableRow key={rule.id}>
                  <TableCell className="font-medium">{rule.name}</TableCell>
                  <TableCell className="text-muted-foreground">{rule.scope}</TableCell>
                  <TableCell className="text-muted-foreground">{rule.metric}</TableCell>
                  <TableCell className="tabular-nums">{rule.threshold}</TableCell>
                  <TableCell className="text-muted-foreground">{rule.action}</TableCell>
                  <TableCell>
                    <ValueBadge value={statusLabel(rule.lastStatus)} variants={STATUS_VARIANTS} />
                  </TableCell>
                  <TableCell>
                    <ValueBadge value={enabledLabel(rule.enabled)} variants={ENABLED_VARIANTS} />
                  </TableCell>
                  <TableCell className="text-right">
                    <AlertToggle id={rule.id} enabled={rule.enabled} />
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
