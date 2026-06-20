import { describe, expect, it } from "vitest"

import { auditToCsv } from "./csv"
import type { AuditEntry } from "./types"

function entry(overrides: Partial<AuditEntry>): AuditEntry {
  return {
    id: "a1",
    actor: "admin",
    method: "POST",
    path: "/v1/accounts",
    status: 200,
    request_id: "req-1",
    created_at: "2026-06-20T00:00:00Z",
    ...overrides,
  }
}

describe("auditToCsv", () => {
  it("emits a header and one row per entry", () => {
    const csv = auditToCsv([
      entry({}),
      entry({ id: "a2", request_id: "req-2" }),
    ])
    const lines = csv.split("\n")
    expect(lines[0]).toBe("time,actor,method,path,status,request_id")
    expect(lines).toHaveLength(3)
    expect(lines[1]).toBe(
      "2026-06-20T00:00:00Z,admin,POST,/v1/accounts,200,req-1"
    )
  })

  it("quotes cells containing commas, quotes, or newlines and doubles inner quotes", () => {
    const csv = auditToCsv([entry({ actor: 'svc,"prod"', path: "/v1/a\nb" })])
    const row = csv.split("\n").slice(1).join("\n")
    expect(row).toContain('"svc,""prod"""')
    expect(row).toContain('"/v1/a\nb"')
  })

  it("returns just the header for no entries", () => {
    expect(auditToCsv([])).toBe("time,actor,method,path,status,request_id")
  })
})
