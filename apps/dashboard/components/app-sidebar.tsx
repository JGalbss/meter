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
  ListBullets,
  Package,
  Plugs,
  Receipt,
  ShieldWarning,
  Tag,
  Wallet,
} from "@phosphor-icons/react"
import Link from "next/link"
import { usePathname } from "next/navigation"
import type { ComponentType } from "react"

import { MeterLogo } from "@/components/brand/logo"
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

interface NavGroup {
  readonly label: string
  readonly items: readonly NavItem[]
}

// Grouped IA: the operator's mental model — what am I looking at, who is spending, how do I bill them,
// and where do I configure the workspace — instead of one flat wall of tabs.
const NAV: readonly NavGroup[] = [
  {
    label: "Overview",
    items: [{ href: "/", label: "Overview", icon: House }],
  },
  {
    label: "Usage",
    items: [
      { href: "/usage", label: "Usage", icon: ChartLineUp },
      { href: "/events", label: "Events", icon: ListBullets },
    ],
  },
  {
    label: "Customers",
    items: [
      { href: "/agents", label: "Agents", icon: Package },
      { href: "/accounts", label: "Accounts", icon: Wallet },
    ],
  },
  {
    label: "Billing",
    items: [
      { href: "/invoices", label: "Invoices", icon: Receipt },
      { href: "/rate-cards", label: "Rate cards", icon: Tag },
      { href: "/simulate", label: "Pricing simulator", icon: Calculator },
      { href: "/alerts", label: "Alert rules", icon: ShieldWarning },
    ],
  },
  {
    label: "Settings",
    items: [
      { href: "/organizations", label: "Organizations", icon: Buildings },
      { href: "/api-keys", label: "API keys", icon: Key },
      { href: "/webhooks", label: "Webhooks", icon: Plugs },
      { href: "/notifications", label: "Notifications", icon: Bell },
      { href: "/audit", label: "Audit log", icon: ClipboardText },
    ],
  },
]

function isActive(pathname: string, href: string): boolean {
  if (href === "/") {
    return pathname === "/"
  }
  return pathname.startsWith(href)
}

export function AppSidebar() {
  const pathname = usePathname()
  return (
    <Sidebar>
      <SidebarHeader>
        <Link href="/" className="px-2 py-1.5">
          <MeterLogo />
        </Link>
      </SidebarHeader>
      <SidebarContent>
        {NAV.map((group) => (
          <SidebarGroup key={group.label}>
            <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {group.items.map((item) => {
                  const Icon = item.icon
                  return (
                    <SidebarMenuItem key={item.href}>
                      <SidebarMenuButton
                        render={<Link href={item.href} prefetch />}
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
        ))}
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
