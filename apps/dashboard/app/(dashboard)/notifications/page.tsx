import { Bell, PlugsConnected } from "@phosphor-icons/react/dist/ssr"
import Link from "next/link"
import { Suspense } from "react"

import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { RevealOnLoad, TableSkeleton } from "@/components/section-skeleton"
import { ValueBadge } from "@/components/value-badge"
import { Card } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { listNotifications, unwrapOr } from "@/lib/meter/client"
import { resolveOrgScope } from "@/lib/meter/org"
import { NotificationActions } from "./notification-actions"

const SEVERITY_VARIANTS = {
  info: "secondary",
  warning: "default",
  critical: "destructive",
} as const
const STATUS_VARIANTS = {
  unread: "default",
  read: "secondary",
  acked: "outline",
} as const

const TABS: ReadonlyArray<{ label: string; value: string | undefined }> = [
  { label: "All", value: undefined },
  { label: "Unread", value: "unread" },
  { label: "Read", value: "read" },
  { label: "Acked", value: "acked" },
]

function tabHref(value: string | undefined): string {
  if (value === undefined) {
    return "/notifications"
  }
  return `/notifications?status=${value}`
}

function tabClass(active: boolean): string {
  if (active) {
    return "bg-secondary text-secondary-foreground"
  }
  return "text-muted-foreground hover:text-foreground"
}

export default async function NotificationsPage({
  searchParams,
}: {
  searchParams: Promise<{ status?: string }>
}) {
  const { status } = await searchParams
  const scope = await resolveOrgScope()

  if (scope.error !== null) {
    return (
      <>
        <PageHeader title="Notifications" />
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
        <PageHeader title="Notifications" />
        <EmptyState
          icon={Bell}
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
        title="Notifications"
        description="Pull, read, and acknowledge alerts."
      />

      <div className="mb-4 flex gap-1">
        {TABS.map((tab) => (
          <Link
            key={tab.label}
            href={tabHref(tab.value)}
            className={`rounded-md px-3 py-1.5 text-sm font-medium transition-colors ${tabClass(status === tab.value)}`}
          >
            {tab.label}
          </Link>
        ))}
      </div>

      <Suspense fallback={<TableSkeleton />}>
        <NotificationsList orgId={orgId} status={status} />
      </Suspense>
    </>
  )
}

async function NotificationsList({
  orgId,
  status,
}: {
  orgId: string
  status: string | undefined
}) {
  const notifications = unwrapOr(await listNotifications(orgId, status), [])

  return (
    <RevealOnLoad>
      <Card className="py-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Severity</TableHead>
              <TableHead>Title</TableHead>
              <TableHead>Type</TableHead>
              <TableHead>Created</TableHead>
              <TableHead>Status</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {notifications.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={6}
                  className="py-10 text-center text-sm text-muted-foreground"
                >
                  No notifications.
                </TableCell>
              </TableRow>
            )}
            {notifications.map((notification) => (
              <TableRow key={notification.id}>
                <TableCell>
                  <ValueBadge
                    value={notification.severity}
                    variants={SEVERITY_VARIANTS}
                  />
                </TableCell>
                <TableCell className="font-medium">
                  {notification.title}
                </TableCell>
                <TableCell className="text-muted-foreground">
                  {notification.type}
                </TableCell>
                <TableCell className="text-muted-foreground">
                  {new Date(notification.createdAt).toLocaleString()}
                </TableCell>
                <TableCell>
                  <ValueBadge
                    value={notification.status}
                    variants={STATUS_VARIANTS}
                  />
                </TableCell>
                <TableCell>
                  <NotificationActions
                    id={notification.id}
                    status={notification.status}
                  />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Card>
    </RevealOnLoad>
  )
}
