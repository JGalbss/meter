"use client"

import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts"

import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart"
import type { DayUsage } from "@/lib/meter/types"

const config = {
  credits: { label: "Credits", color: "var(--chart-1)" },
} satisfies ChartConfig

export function UsageChart({ data }: { data: readonly DayUsage[] }) {
  const points = data.map((point) => ({
    day: point.day,
    credits: Number(point.total_credits),
  }))

  return (
    <ChartContainer config={config} className="h-[300px] w-full">
      <AreaChart data={points} margin={{ left: 12, right: 12 }}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="day" tickLine={false} axisLine={false} tickMargin={8} />
        <YAxis tickLine={false} axisLine={false} width={48} />
        <ChartTooltip content={<ChartTooltipContent />} />
        <Area
          dataKey="credits"
          type="monotone"
          fill="var(--color-credits)"
          fillOpacity={0.2}
          stroke="var(--color-credits)"
        />
      </AreaChart>
    </ChartContainer>
  )
}
