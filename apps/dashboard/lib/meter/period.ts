//! The billing period the dashboard sums an invoice over: month-to-date in UTC — the deterministic
//! window the engine sums the ledger over. Shared by the invoice page and its PDF route.

export interface BillingPeriod {
  readonly start: string
  readonly end: string
  readonly label: string
}

function formatDay(date: Date): string {
  return date.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  })
}

export function monthToDate(): BillingPeriod {
  const now = new Date()
  const start = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1))
  return {
    start: start.toISOString(),
    end: now.toISOString(),
    label: `${formatDay(start)} – ${formatDay(now)}`,
  }
}
