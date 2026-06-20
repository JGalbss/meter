"use client";

import { CaretDown, Check } from "@phosphor-icons/react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { Organization } from "@/lib/meter/types";

function activeOrg(orgs: readonly Organization[], current: string | null): Organization | null {
  if (current !== null) {
    const match = orgs.find((org) => org.id === current);
    if (match !== undefined) {
      return match;
    }
  }
  return orgs[0] ?? null;
}

export function OrgSwitcher({ orgs }: { orgs: readonly Organization[] }) {
  const router = useRouter();
  const pathname = usePathname();
  const current = useSearchParams().get("org");
  const active = activeOrg(orgs, current);

  if (active === null) {
    return <span className="text-sm text-muted-foreground">No organizations</span>;
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="outline" size="sm" className="gap-2" />}>
        <span className="max-w-40 truncate">{active.name}</span>
        <CaretDown size={14} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuLabel>Organizations</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {orgs.map((org) => (
          <DropdownMenuItem
            key={org.id}
            onClick={() => router.push(`${pathname}?org=${org.id}`)}
            className="justify-between"
          >
            <span className="truncate">{org.name}</span>
            {org.id === active.id && <Check size={14} />}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
