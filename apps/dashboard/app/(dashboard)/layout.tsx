import { redirect } from "next/navigation";
import { type ReactNode, Suspense } from "react";

import { AppSidebar } from "@/components/app-sidebar";
import { OrgSwitcher } from "@/components/org-switcher";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { hasValidSession } from "@/lib/auth/session";
import { listOrganizations, unwrapOr } from "@/lib/meter/client";
import { logoutAction } from "./actions";

export default async function DashboardLayout({ children }: { children: ReactNode }) {
  if (!(await hasValidSession())) {
    redirect("/login");
  }

  const orgs = unwrapOr(await listOrganizations(), []);
  return (
    <SidebarProvider>
      <Suspense fallback={null}>
        <AppSidebar />
      </Suspense>
      <SidebarInset>
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger />
          <Separator orientation="vertical" className="mr-1 h-4" />
          <Suspense fallback={null}>
            <OrgSwitcher orgs={orgs} />
          </Suspense>
          <form action={logoutAction} className="ml-auto">
            <Button type="submit" variant="ghost" size="sm">
              Sign out
            </Button>
          </form>
        </header>
        <main className="flex-1 p-6">{children}</main>
      </SidebarInset>
    </SidebarProvider>
  );
}
