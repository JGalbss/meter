"use client";

import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";

import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import type { ModelUsage } from "@/lib/meter/types";

const config = {
  credits: { label: "Credits", color: "var(--chart-1)" },
} satisfies ChartConfig;

export function ModelUsageChart({ data }: { data: readonly ModelUsage[] }) {
  const points = data.map((usage) => ({ model: usage.model, credits: usage.credits }));

  return (
    <ChartContainer config={config} className="h-[300px] w-full">
      <BarChart data={points} layout="vertical" margin={{ left: 12, right: 12 }}>
        <CartesianGrid horizontal={false} />
        <XAxis type="number" tickLine={false} axisLine={false} tickMargin={8} />
        <YAxis
          type="category"
          dataKey="model"
          tickLine={false}
          axisLine={false}
          width={140}
          className="font-mono text-xs"
        />
        <ChartTooltip content={<ChartTooltipContent />} />
        <Bar dataKey="credits" fill="var(--color-credits)" radius={4} />
      </BarChart>
    </ChartContainer>
  );
}
