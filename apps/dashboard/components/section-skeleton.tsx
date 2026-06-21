import type { ReactNode } from "react"

import { Card } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"

const ROWS = ["a", "b", "c", "d", "e", "f"]

// A section-level loading placeholder for a page's data region. Used as a <Suspense> fallback so the
// page shell and header stay put while only the table area swaps — instead of a full-page skeleton on
// every navigation. Pairs with <RevealOnLoad> for the cross-blur reveal of the real content.
export function TableSkeleton({ rows = 6 }: { rows?: number }) {
  return (
    <Card className="gap-0 py-0">
      <div className="space-y-3 p-6">
        {ROWS.slice(0, rows).map((row) => (
          <Skeleton key={row} className="h-10 w-full" />
        ))}
      </div>
    </Card>
  )
}

// A row of metric-card placeholders, for pages that lead with summary stats.
export function StatsSkeleton({ count = 3 }: { count?: number }) {
  return (
    <div className="grid gap-4 sm:grid-cols-3">
      {ROWS.slice(0, count).map((row) => (
        <Card key={row} size="sm">
          <div className="space-y-2 px-5">
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-8 w-20" />
          </div>
        </Card>
      ))}
    </div>
  )
}

// Wraps streamed-in content so it fades + un-blurs once on mount (the content half of the
// transitions.dev skeleton-reveal). Keep it directly inside the <Suspense> boundary.
export function RevealOnLoad({ children }: { children: ReactNode }) {
  return <div className="t-reveal">{children}</div>
}
