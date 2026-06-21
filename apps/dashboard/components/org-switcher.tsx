"use client"

import { Buildings, CaretDown, Check } from "@phosphor-icons/react"
import Link from "next/link"
import { useTransition } from "react"

import { selectActiveOrgAction } from "@/app/(dashboard)/actions"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { Organization } from "@/lib/meter/types"

function findActive(
  orgs: readonly Organization[],
  activeOrgId: string | null
): Organization | null {
  if (activeOrgId !== null) {
    const match = orgs.find((org) => org.id === activeOrgId)
    if (match !== undefined) {
      return match
    }
  }
  return orgs[0] ?? null
}

export function OrgSwitcher({
  orgs,
  activeOrgId,
}: {
  orgs: readonly Organization[]
  activeOrgId: string | null
}) {
  const [isPending, startTransition] = useTransition()
  const active = findActive(orgs, activeOrgId)

  if (active === null) {
    return (
      <Button
        variant="outline"
        size="sm"
        className="gap-2"
        render={<Link href="/organizations" />}
      >
        <Buildings size={14} />
        Set up organization
      </Button>
    )
  }

  const select = (orgId: string) => {
    if (orgId === active.id) {
      return
    }
    startTransition(() => {
      void selectActiveOrgAction(orgId)
    })
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="outline"
            size="sm"
            className="gap-2"
            disabled={isPending}
          />
        }
      >
        <Buildings size={14} className="text-muted-foreground" />
        {/* transitions.dev "text states swap": the org name swaps in place while switching. */}
        <span className="max-w-40 truncate" data-pending={isPending}>
          {active.name}
        </span>
        <CaretDown size={14} className="text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuLabel>Organizations</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {orgs.map((org) => (
          <DropdownMenuItem
            key={org.id}
            onClick={() => select(org.id)}
            className="justify-between"
          >
            <span className="truncate">{org.name}</span>
            {org.id === active.id && <Check size={14} />}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem render={<Link href="/organizations" />}>
          <Buildings size={14} />
          Manage organizations
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
