import { describe, expect, it } from "vitest"

import { buildInvoicePdf } from "./invoice-pdf"

function pdfHeader(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes.slice(0, 5))
}

describe("buildInvoicePdf", () => {
  it("renders a valid PDF from an engine invoice", async () => {
    const bytes = await buildInvoicePdf({
      invoice: { account_id: "acc-123", total_credits: "1234.5", entries: 42 },
      days: [
        { day: "2026-06-01", total_credits: "12.5", entry_count: 3 },
        { day: "2026-06-02", total_credits: "0", entry_count: 0 },
      ],
      periodLabel: "Jun 1, 2026 – Jun 21, 2026",
      title: "Account acc-123",
    })
    expect(pdfHeader(bytes)).toBe("%PDF-")
    expect(bytes.length).toBeGreaterThan(500)
  })

  it("renders a valid PDF for an empty period", async () => {
    const bytes = await buildInvoicePdf({
      invoice: { account_id: "acc-0", total_credits: "0", entries: 0 },
      days: [],
      periodLabel: "Jun 2026",
      title: "Account acc-0",
    })
    expect(pdfHeader(bytes)).toBe("%PDF-")
  })

  it("paginates a long daily breakdown without throwing", async () => {
    const days = Array.from({ length: 90 }, (_, index) => ({
      day: `2026-06-${String((index % 30) + 1).padStart(2, "0")}`,
      total_credits: String(index),
      entry_count: index,
    }))
    const bytes = await buildInvoicePdf({
      invoice: { account_id: "acc-long", total_credits: "4005", entries: 4005 },
      days,
      periodLabel: "long range",
      title: "Account acc-long",
    })
    expect(pdfHeader(bytes)).toBe("%PDF-")
  })
})
