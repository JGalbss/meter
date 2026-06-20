import { Bell, Buildings, Plugs, PlugsConnected, ShieldWarning } from "@phosphor-icons/react/dist/ssr";
import Link from "next/link";
import type { ComponentType } from "react";

import { EmptyState } from "@/components/empty-state";
import { PageHeader } from "@/components/page-header";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { listAlertRules, listNotifications, listWebhooks, unwrapOr } from "@/lib/meter/client";
import { resolveOrgScope } from "@/lib/meter/org";

export const dynamic = "force-dynamic";

interface Stat {
  readonly label: string;
  readonly value: number;
  readonly href: string;
  readonly icon: ComponentType<{ size?: number; className?: string }>;
}

export default async function OverviewPage({
  searchParams,
}: {
  searchParams: Promise<{ org?: string }>;
}) {
  const { org } = await searchParams;
  const scope = await resolveOrgScope(org);

  if (scope.error !== null) {
    return (
      <>
        <PageHeader title="Overview" />
        <EmptyState
          icon={PlugsConnected}
          title="Control plane unreachable"
          message={`${scope.error}. Start it with \`pnpm --filter @meter/control-plane run dev\`.`}
        />
      </>
    );
  }

  if (scope.activeOrg === null) {
    return (
      <>
        <PageHeader title="Overview" description="Your metering console." />
        <EmptyState
          icon={Buildings}
          title="No organizations yet"
          message="Create an organization in the control plane to get started."
        />
      </>
    );
  }

  const orgId = scope.activeOrg.id;
  const [unread, alerts, webhooks, recent] = await Promise.all([
    listNotifications(orgId, "unread"),
    listAlertRules(orgId),
    listWebhooks(orgId),
    listNotifications(orgId),
  ]);

  const stats: readonly Stat[] = [
    { label: "Unread notifications", value: unwrapOr(unread, []).length, href: "/notifications", icon: Bell },
    { label: "Alert rules", value: unwrapOr(alerts, []).length, href: "/alerts", icon: ShieldWarning },
    { label: "Webhooks", value: unwrapOr(webhooks, []).length, href: "/webhooks", icon: Plugs },
    { label: "Organizations", value: scope.orgs.length, href: "/organizations", icon: Buildings },
  ];
  const recentNotifications = unwrapOr(recent, []).slice(0, 5);

  return (
    <>
      <PageHeader title="Overview" description={`Metering console for ${scope.activeOrg.name}.`} />
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {stats.map((stat) => {
          const Icon = stat.icon;
          return (
            <Link key={stat.label} href={`${stat.href}?org=${orgId}`} className="group">
              <Card className="transition-colors group-hover:border-primary/40">
                <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                  <CardTitle className="text-sm font-medium text-muted-foreground">
                    {stat.label}
                  </CardTitle>
                  <Icon size={18} className="text-muted-foreground" />
                </CardHeader>
                <CardContent>
                  <p className="font-heading text-3xl font-semibold tabular-nums">{stat.value}</p>
                </CardContent>
              </Card>
            </Link>
          );
        })}
      </div>

      <Card className="mt-6">
        <CardHeader>
          <CardTitle>Recent notifications</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {recentNotifications.length === 0 && (
            <p className="text-sm text-muted-foreground">No notifications yet.</p>
          )}
          {recentNotifications.map((notification) => (
            <div
              key={notification.id}
              className="flex items-center justify-between border-b pb-3 last:border-0 last:pb-0"
            >
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">{notification.title}</p>
                <p className="text-xs text-muted-foreground">
                  {notification.type} · {new Date(notification.createdAt).toLocaleString()}
                </p>
              </div>
              <span className="text-xs text-muted-foreground">{notification.status}</span>
            </div>
          ))}
        </CardContent>
      </Card>
    </>
  );
}
