import type { ReactNode } from "react";

// The meter brand mark: a tape measure seen side-on — a rounded case with its spool, and the steel
// blade pulled out, ruled with graduations and ending in the hook. Stroke-based on currentColor so it
// inherits the sidebar's `.brand-mark` color.
export function MeterMark({ size = 22 }: { size?: number }): ReactNode {
  return (
    <svg
      className="brand-mark"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="2" y="6.5" width="10" height="11" rx="3" />
      <circle cx="7" cy="12" r="2.4" />
      <path d="M12 9.5 H20.5 V12.5" />
      <path d="M15 9.5 V11.4 M17.75 9.5 V12" />
    </svg>
  );
}
