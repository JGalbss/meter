import { ChartLineUp } from "@phosphor-icons/react/dist/ssr";

import { EmptyState } from "@/components/empty-state";
import { PageHeader } from "@/components/page-header";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { unwrapOr } from "@/lib/meter/client";
import { fetchUsageByDay } from "@/lib/meter/engine";
import { AccountForm } from "./account-form";
import { UsageChartLazy } from "./usage-chart-lazy";

export const dynamic = "force-dynamic";

const DAY_MS = 86_400_000;

function thirtyDayWindow(): { start: string; end: string } {
  const now = new Date();
  const start = new Date(now.getTime() - 30 * DAY_MS);
  return { start: start.toISOString(), end: now.toISOString() };
}

export default async function UsagePage({
  searchParams,
}: {
  searchParams: Promise<{ account?: string }>;
}) {
  const { account } = await searchParams;

  if (account === undefined || account.length === 0) {
    return (
      <>
        <PageHeader
          title="Usage"
          description="Credit burn over time, per engine account."
          action={<AccountForm initial="" />}
        />
        <EmptyState
          icon={ChartLineUp}
          title="Choose an account"
          message="Enter an engine account id to chart its daily credit usage."
        />
      </>
    );
  }

  const { start, end } = thirtyDayWindow();
  const series = unwrapOr(await fetchUsageByDay(account, start, end), []);

  return (
    <>
      <PageHeader
        title="Usage"
        description="Credit burn over the last 30 days."
        action={<AccountForm initial={account} />}
      />
      <Card>
        <CardHeader>
          <CardTitle className="font-mono text-sm">{account}</CardTitle>
        </CardHeader>
        <CardContent>
          {series.length === 0 && (
            <p className="py-10 text-center text-sm text-muted-foreground">
              No usage in this window (or the engine is unreachable).
            </p>
          )}
          {series.length > 0 && <UsageChartLazy data={series} />}
        </CardContent>
      </Card>
    </>
  );
}
