import {
  ArrowRight,
  Bell,
  Buildings,
  Plugs,
  PlugsConnected,
  ShieldWarning,
} from "@phosphor-icons/react/dist/ssr"
import Link from "next/link"
import { redirect } from "next/navigation"
import type { ComponentType } from "react"
import { Suspense } from "react"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { RevealOnLoad, StatsSkeleton } from "@/components/section-skeleton"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  listAlertRules,
  listNotifications,
  listWebhooks,
  unwrapOr,
} from "@/lib/meter/client"
import { fetchUsageByModel } from "@/lib/meter/engine"
import { resolveOrgScope } from "@/lib/meter/org"
import { isOnboardingDismissed } from "@/lib/onboarding"

interface Stat {
  readonly label: string
  readonly value: number
  readonly href: string
  readonly icon: ComponentType<{ size?: number; className?: string }>
}

export default async function OverviewPage() {
  const scope = await resolveOrgScope()

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
    )
  }

  if (scope.activeOrg === null) {
    // Fresh deployment: route straight into the guided setup (unless it was already skipped).
    if (!(await isOnboardingDismissed())) {
      redirect("/onboarding")
    }
    return (
      <>
        <PageHeader title="Overview" description="Your metering console." />
        <EmptyState
          icon={Buildings}
          title="No organizations yet"
          message="Spin up your first workspace, agent, and key in a guided setup."
          action={
            <Button render={<Link href="/onboarding" />}>
              Start setup
              <ArrowRight />
            </Button>
          }
        />
      </>
    )
  }

  const orgId = scope.activeOrg.id

  return (
    <>
      <PageHeader
        title="Overview"
        description={`Metering console for ${scope.activeOrg.name}.`}
      />
      <Suspense fallback={<StatsSkeleton count={4} />}>
        <OverviewSections orgId={orgId} orgCount={scope.orgs.length} />
      </Suspense>
    </>
  )
}

async function OverviewSections({
  orgId,
  orgCount,
}: {
  orgId: string
  orgCount: number
}) {
  const [unread, alerts, webhooks, recent, usageByModel] = await Promise.all([
    listNotifications(orgId, "unread"),
    listAlertRules(orgId),
    listWebhooks(orgId),
    listNotifications(orgId),
    fetchUsageByModel(orgId),
  ])
  const models = unwrapOr(usageByModel, [])
  const topModels = models.slice(0, 5)

  const stats: readonly Stat[] = [
    {
      label: "Unread notifications",
      value: unwrapOr(unread, []).length,
      href: "/notifications",
      icon: Bell,
    },
    {
      label: "Alert rules",
      value: unwrapOr(alerts, []).length,
      href: "/alerts",
      icon: ShieldWarning,
    },
    {
      label: "Webhooks",
      value: unwrapOr(webhooks, []).length,
      href: "/webhooks",
      icon: Plugs,
    },
    {
      label: "Organizations",
      value: orgCount,
      href: "/organizations",
      icon: Buildings,
    },
  ]
  const recentNotifications = unwrapOr(recent, []).slice(0, 5)

  return (
    <RevealOnLoad>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {stats.map((stat) => {
          const Icon = stat.icon
          return (
            <Link key={stat.label} href={stat.href} className="group">
              <Card className="transition-colors group-hover:border-primary/40">
                <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                  <CardTitle className="text-sm font-medium text-muted-foreground">
                    {stat.label}
                  </CardTitle>
                  <Icon size={18} className="text-muted-foreground" />
                </CardHeader>
                <CardContent>
                  <p className="font-heading text-3xl font-semibold tabular-nums">
                    {stat.value}
                  </p>
                </CardContent>
              </Card>
            </Link>
          )
        })}
      </div>

      <Card className="mt-6">
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>Top models by spend</CardTitle>
          <Link
            href="/usage"
            className="text-sm font-normal text-muted-foreground hover:text-foreground"
          >
            View usage →
          </Link>
        </CardHeader>
        <CardContent className="space-y-3">
          {topModels.length === 0 && (
            <p className="text-sm text-muted-foreground">
              No usage recorded yet.
            </p>
          )}
          {topModels.map((model) => (
            <div
              key={model.model}
              className="flex items-center justify-between border-b pb-3 last:border-0 last:pb-0"
            >
              <span className="font-mono text-sm">{model.model}</span>
              <span className="text-sm text-muted-foreground tabular-nums">
                {model.credits.toLocaleString()} credits
              </span>
            </div>
          ))}
        </CardContent>
      </Card>

      <Card className="mt-6">
        <CardHeader>
          <CardTitle>Recent notifications</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {recentNotifications.length === 0 && (
            <p className="text-sm text-muted-foreground">
              No notifications yet.
            </p>
          )}
          {recentNotifications.map((notification) => (
            <div
              key={notification.id}
              className="flex items-center justify-between border-b pb-3 last:border-0 last:pb-0"
            >
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">
                  {notification.title}
                </p>
                <p className="text-xs text-muted-foreground">
                  {notification.type} ·{" "}
                  {new Date(notification.createdAt).toLocaleString()}
                </p>
              </div>
              <span className="text-xs text-muted-foreground">
                {notification.status}
              </span>
            </div>
          ))}
        </CardContent>
      </Card>
    </RevealOnLoad>
  )
}
