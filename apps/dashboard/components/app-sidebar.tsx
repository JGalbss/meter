"use client"

import {
  Bell,
  BookOpen,
  Buildings,
  Calculator,
  ChartLineUp,
  ClipboardText,
  House,
  Key,
  Lightning,
  ListBullets,
  Package,
  Plugs,
  Receipt,
  ShieldWarning,
  Tag,
  Wallet,
} from "@phosphor-icons/react"
import Link from "next/link"
import { usePathname, useSearchParams } from "next/navigation"
import type { ComponentType } from "react"

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

// The docs live in a separate app; self-hosters point this at their deployed docs site. The fallback
// is the documentation in the repository, which is always valid.
const DOCS_URL =
  process.env.NEXT_PUBLIC_DOCS_URL ??
  "https://github.com/JGalbss/meter/tree/main/docs"

interface NavItem {
  readonly href: string
  readonly label: string
  readonly icon: ComponentType<{ size?: number }>
}

const NAV: readonly NavItem[] = [
  { href: "/", label: "Overview", icon: House },
  { href: "/usage", label: "Usage", icon: ChartLineUp },
  { href: "/events", label: "Events", icon: ListBullets },
  { href: "/accounts", label: "Accounts", icon: Wallet },
  { href: "/invoices", label: "Invoices", icon: Receipt },
  { href: "/rate-cards", label: "Rate cards", icon: Tag },
  { href: "/simulate", label: "Pricing simulator", icon: Calculator },
  { href: "/organizations", label: "Organizations", icon: Buildings },
  { href: "/products", label: "Products", icon: Package },
  { href: "/notifications", label: "Notifications", icon: Bell },
  { href: "/alerts", label: "Alert rules", icon: ShieldWarning },
  { href: "/webhooks", label: "Webhooks", icon: Plugs },
  { href: "/api-keys", label: "API keys", icon: Key },
  { href: "/audit", label: "Audit log", icon: ClipboardText },
]

function isActive(pathname: string, href: string): boolean {
  if (href === "/") {
    return pathname === "/"
  }
  return pathname.startsWith(href)
}

function withOrg(href: string, org: string | null): string {
  if (org === null) {
    return href
  }
  return `${href}?org=${org}`
}

export function AppSidebar() {
  const pathname = usePathname()
  const org = useSearchParams().get("org")
  return (
    <Sidebar>
      <SidebarHeader>
        <div className="flex items-center gap-2 px-2 py-1.5">
          <div className="flex size-8 items-center justify-center rounded-md bg-primary text-primary-foreground">
            <Lightning size={18} weight="fill" />
          </div>
          <span className="font-heading text-lg font-semibold tracking-tight">
            meter
          </span>
        </div>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>Console</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {NAV.map((item) => {
                const Icon = item.icon
                return (
                  <SidebarMenuItem key={item.href}>
                    <SidebarMenuButton
                      render={<Link href={withOrg(item.href, org)} />}
                      isActive={isActive(pathname, item.href)}
                      tooltip={item.label}
                    >
                      <Icon size={18} />
                      <span>{item.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                )
              })}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              render={<a href={DOCS_URL} target="_blank" rel="noreferrer" />}
              tooltip="Documentation"
            >
              <BookOpen size={18} />
              <span>Documentation</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
  )
}
