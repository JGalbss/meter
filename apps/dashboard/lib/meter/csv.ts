import type { AuditEntry } from "./types"

const AUDIT_HEADERS = [
  "time",
  "actor",
  "method",
  "path",
  "status",
  "request_id",
] as const

// Quote a CSV cell only when it contains a comma, quote, or newline (RFC 4180), doubling inner quotes.
function csvCell(value: string): string {
  if (/[",\n\r]/.test(value)) {
    return `"${value.replace(/"/g, '""')}"`
  }
  return value
}

/** Render audit entries as RFC 4180 CSV (header + one row per entry). */
export function auditToCsv(entries: readonly AuditEntry[]): string {
  const rows = entries.map((entry) =>
    [
      entry.created_at,
      entry.actor,
      entry.method,
      entry.path,
      String(entry.status),
      entry.request_id,
    ]
      .map(csvCell)
      .join(",")
  )
  return [AUDIT_HEADERS.join(","), ...rows].join("\n")
}
