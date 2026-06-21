import type { ReactNode } from "react";

// The meter brand mark: a tape measure — a rounded case with the blade pulled out over the top,
// ruled with ticks. Stroke-based on currentColor so it inherits the sidebar's `.brand-mark` color.
export function MeterMark({ size = 22 }: { size?: number }): ReactNode {
  return (
    <svg
      className="brand-mark"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="2.25" y="9.5" width="12.25" height="12" rx="3.5" />
      <circle cx="8.375" cy="15.5" r="2.25" />
      <path d="M11.5 9.5 V6 a3 3 0 0 1 3 -3 H21.25" />
      <path d="M15.75 3 v3 M18 3 v2 M20.25 3 v3" />
    </svg>
  );
}
