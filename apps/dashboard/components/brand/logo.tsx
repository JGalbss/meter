import type { ComponentProps } from "react"

import { cn } from "@/lib/utils"

// The meter brand mark: a tape measure — a rounded case with the blade pulled out and bent over the
// top, ruled with tick marks. Stroke-based on `currentColor`, so it inherits text color (mono on
// light/dark, brand when placed on a `text-primary-foreground` surface) and stays crisp at 16–32px.
export function MeterMark({
  size = 24,
  className,
  ...props
}: { size?: number } & ComponentProps<"svg">) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={className}
      {...props}
    >
      {/* The case. */}
      <rect x="2.25" y="9.5" width="12.25" height="12" rx="3.5" />
      {/* The spool the blade winds onto. */}
      <circle cx="8.375" cy="15.5" r="2.25" />
      {/* The blade, pulled from the case and bent over the top corner. */}
      <path d="M11.5 9.5 V6 a3 3 0 0 1 3 -3 H21.25" />
      {/* Ruler ticks along the extended blade (long–short–long). */}
      <path d="M15.75 3 v3 M18 3 v2 M20.25 3 v3" />
    </svg>
  )
}

// The full lockup: brand mark in its accent tile + the wordmark. Used in the dashboard sidebar header
// and the onboarding hero.
export function MeterLogo({
  className,
  markSize = 18,
  ...props
}: { markSize?: number } & ComponentProps<"div">) {
  return (
    <div className={cn("flex items-center gap-2", className)} {...props}>
      <span className="flex size-8 items-center justify-center rounded-md bg-primary text-primary-foreground">
        <MeterMark size={markSize} />
      </span>
      <span className="font-heading text-lg font-semibold tracking-tight">
        meter
      </span>
    </div>
  )
}
