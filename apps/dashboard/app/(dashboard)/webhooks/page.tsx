import { PlugsConnected } from "@phosphor-icons/react/dist/ssr"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { ValueBadge } from "@/components/value-badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  listWebhookDeliveries,
  listWebhooks,
  unwrapOr,
} from "@/lib/meter/client"
import { resolveOrgScope } from "@/lib/meter/org"
import { RegisterWebhookDialog } from "./register-webhook-dialog"
import { WebhookToggle } from "./webhook-toggle"

export const dynamic = "force-dynamic"

const ENABLED_VARIANTS = { enabled: "default", disabled: "outline" } as const
const DELIVERY_VARIANTS = {
  delivered: "secondary",
  failed: "destructive",
} as const

function eventsLabel(eventTypes: readonly string[]): string {
  if (eventTypes.length === 0) {
    return "all events"
  }
  return eventTypes.join(", ")
}

function enabledLabel(enabled: boolean): string {
  if (enabled) {
    return "enabled"
  }
  return "disabled"
}

function dash(value: number | null): string {
  if (value === null) {
    return "—"
  }
  return String(value)
}

export default async function WebhooksPage({
  searchParams,
}: {
  searchParams: Promise<{ org?: string }>
}) {
  const { org } = await searchParams
  const scope = await resolveOrgScope(org)

  if (scope.error !== null) {
    return (
      <>
        <PageHeader title="Webhooks" />
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
        <PageHeader title="Webhooks" />
        <EmptyState
          icon={PlugsConnected}
          title="No organization"
          message="Create an organization first."
        />
      </>
    )
  }

  const orgId = scope.activeOrg.id
  const [webhooks, deliveries] = await Promise.all([
    listWebhooks(orgId),
    listWebhookDeliveries(orgId),
  ])
  const endpoints = unwrapOr(webhooks, [])
  const log = unwrapOr(deliveries, [])

  return (
    <>
      <PageHeader
        title="Webhooks"
        description="Signed, retried event delivery with a dead-letter log."
        action={<RegisterWebhookDialog orgId={orgId} />}
      />

      <Card>
        <CardHeader>
          <CardTitle>Endpoints</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>URL</TableHead>
                <TableHead>Events</TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {endpoints.length === 0 && (
                <TableRow>
                  <TableCell
                    colSpan={4}
                    className="py-10 text-center text-sm text-muted-foreground"
                  >
                    No webhooks registered.
                  </TableCell>
                </TableRow>
              )}
              {endpoints.map((webhook) => (
                <TableRow key={webhook.id}>
                  <TableCell className="max-w-80 truncate font-mono text-xs">
                    {webhook.url}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {eventsLabel(webhook.eventTypes)}
                  </TableCell>
                  <TableCell>
                    <ValueBadge
                      value={enabledLabel(webhook.enabled)}
                      variants={ENABLED_VARIANTS}
                    />
                  </TableCell>
                  <TableCell className="text-right">
                    <WebhookToggle id={webhook.id} enabled={webhook.enabled} />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card className="mt-6">
        <CardHeader>
          <CardTitle>Recent deliveries</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Event</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Response</TableHead>
                <TableHead>Attempts</TableHead>
                <TableHead>When</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {log.length === 0 && (
                <TableRow>
                  <TableCell
                    colSpan={5}
                    className="py-10 text-center text-sm text-muted-foreground"
                  >
                    No deliveries yet.
                  </TableCell>
                </TableRow>
              )}
              {log.map((delivery) => (
                <TableRow key={delivery.id}>
                  <TableCell className="font-medium">
                    {delivery.event}
                  </TableCell>
                  <TableCell>
                    <ValueBadge
                      value={delivery.status}
                      variants={DELIVERY_VARIANTS}
                    />
                  </TableCell>
                  <TableCell className="tabular-nums">
                    {dash(delivery.responseStatus)}
                  </TableCell>
                  <TableCell className="tabular-nums">
                    {delivery.attempts}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {new Date(delivery.createdAt).toLocaleString()}
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
