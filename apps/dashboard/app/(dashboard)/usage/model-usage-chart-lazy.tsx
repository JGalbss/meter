"use client"

import dynamic from "next/dynamic"

import type { ModelUsage } from "@/lib/meter/types"

// Load recharts (heavy) on demand, client-side only, so it never ships in the initial bundle.
const ModelUsageChart = dynamic(
  () => import("./model-usage-chart").then((module) => module.ModelUsageChart),
  {
    ssr: false,
    loading: () => (
      <div className="h-[300px] w-full animate-pulse rounded-md bg-muted" />
    ),
  }
)

export function ModelUsageChartLazy({ data }: { data: readonly ModelUsage[] }) {
  return <ModelUsageChart data={data} />
}
