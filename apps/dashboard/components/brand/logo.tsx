import type { ComponentProps } from "react"

import { cn } from "@/lib/utils"

// The meter brand mark: a tape measure seen side-on — a rounded case with its spool, and the steel
// blade pulled straight out, ruled with graduations and ending in the hook. Stroke-based on
// `currentColor`, so it inherits text color (mono on light/dark, brand when placed on a
// `text-primary-foreground` surface) and stays crisp at 16–32px.
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
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={className}
      {...props}
    >
      {/* The case. */}
      <rect x="2" y="6.5" width="10" height="11" rx="3" />
      {/* The spool the blade winds onto. */}
      <circle cx="7" cy="12" r="2.4" />
      {/* The blade pulled out of the case, ending in the hook. */}
      <path d="M12 9.5 H20.5 V12.5" />
      {/* Ruler graduations along the blade. */}
      <path d="M15 9.5 V11.4 M17.75 9.5 V12" />
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
