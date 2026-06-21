//! Download an account's current statement as a PDF. The engine computes the numbers; this route only
//! renders them (money-truth stays in the engine, ADR 0001).

import { hasValidSession } from "@/lib/auth/session"
import { unwrapOr } from "@/lib/meter/client"
import { fetchInvoice, fetchUsageByDay } from "@/lib/meter/engine"
import { buildInvoicePdf } from "@/lib/meter/invoice-pdf"
import { monthToDate } from "@/lib/meter/period"

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ account: string }> }
): Promise<Response> {
  if (!(await hasValidSession())) {
    return new Response("unauthorized", { status: 401 })
  }

  const { account } = await params
  const period = monthToDate()

  const invoiceResult = await fetchInvoice(account, period.start, period.end)
  if (!invoiceResult.ok) {
    return new Response("statement unavailable", { status: 502 })
  }
  const days = unwrapOr(
    await fetchUsageByDay(account, period.start, period.end),
    []
  )

  const pdf = await buildInvoicePdf({
    invoice: invoiceResult.data,
    days,
    periodLabel: period.label,
    title: `Account ${account}`,
  })

  // new Uint8Array(pdf) gives a clean ArrayBuffer-backed body for the Response.
  return new Response(new Uint8Array(pdf), {
    headers: {
      "content-type": "application/pdf",
      "content-disposition": `attachment; filename="meter-statement-${account}.pdf"`,
      "cache-control": "no-store",
    },
  })
}
