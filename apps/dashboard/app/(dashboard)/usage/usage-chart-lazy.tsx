"use client"

import dynamic from "next/dynamic"

import type { DayUsage } from "@/lib/meter/types"

// Load recharts (heavy) on demand, client-side only, so it never ships in the initial bundle.
const UsageChart = dynamic(
  () => import("./usage-chart").then((module) => module.UsageChart),
  {
    ssr: false,
    loading: () => (
      <div className="h-[300px] w-full animate-pulse rounded-md bg-muted" />
    ),
  }
)

export function UsageChartLazy({ data }: { data: readonly DayUsage[] }) {
  return <UsageChart data={data} />
}
