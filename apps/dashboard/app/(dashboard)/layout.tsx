import type { ReactNode } from "react";

import { AppSidebar } from "@/components/app-sidebar";
import { OrgSwitcher } from "@/components/org-switcher";
import { Separator } from "@/components/ui/separator";
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { listOrganizations, unwrapOr } from "@/lib/meter/client";

export default async function DashboardLayout({ children }: { children: ReactNode }) {
  const orgs = unwrapOr(await listOrganizations(), []);
  return (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset>
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger />
          <Separator orientation="vertical" className="mr-1 h-4" />
          <OrgSwitcher orgs={orgs} />
        </header>
        <main className="flex-1 p-6">{children}</main>
      </SidebarInset>
    </SidebarProvider>
  );
}
