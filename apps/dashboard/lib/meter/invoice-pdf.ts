//! Render an engine invoice to a PDF. This is presentation only — it draws the engine's numbers
//! verbatim and never re-sums money (the engine is the sole money authority, ADR 0001).

import {
  PDFDocument,
  type PDFFont,
  type PDFPage,
  StandardFonts,
  rgb,
} from "pdf-lib"

import type { DayUsage, Invoice } from "./types"

const PAGE: readonly [number, number] = [612, 792] // US Letter, points
const MARGIN = 56
const INK = rgb(0.09, 0.09, 0.11)
const MUTED = rgb(0.42, 0.42, 0.47)
const RULE = rgb(0.85, 0.85, 0.88)
const BRAND = rgb(0.31, 0.275, 0.898) // indigo, matches the dashboard primary

function formatCredits(value: string): string {
  const parsed = Number(value)
  if (Number.isNaN(parsed)) {
    return value
  }
  return parsed.toLocaleString("en-US", { maximumFractionDigits: 4 })
}

interface Fonts {
  readonly regular: PDFFont
  readonly bold: PDFFont
}

interface Cursor {
  y: number
}

function drawDivider(page: PDFPage, y: number): void {
  page.drawLine({
    start: { x: MARGIN, y },
    end: { x: PAGE[0] - MARGIN, y },
    thickness: 1,
    color: RULE,
  })
}

// One right-aligned cell.
function drawRight(
  page: PDFPage,
  text: string,
  rightX: number,
  y: number,
  font: PDFFont,
  size: number,
  color = INK
): void {
  const width = font.widthOfTextAtSize(text, size)
  page.drawText(text, { x: rightX - width, y, size, font, color })
}

export interface InvoicePdfInput {
  readonly invoice: Invoice
  readonly days: readonly DayUsage[]
  readonly periodLabel: string
  readonly title: string
}

export async function buildInvoicePdf(
  input: InvoicePdfInput
): Promise<Uint8Array> {
  const doc = await PDFDocument.create()
  const fonts: Fonts = {
    regular: await doc.embedFont(StandardFonts.Helvetica),
    bold: await doc.embedFont(StandardFonts.HelveticaBold),
  }
  const right = PAGE[0] - MARGIN
  let page = doc.addPage([PAGE[0], PAGE[1]])
  const cursor: Cursor = { y: PAGE[1] - MARGIN }

  // Masthead.
  page.drawText("meter", {
    x: MARGIN,
    y: cursor.y - 6,
    size: 22,
    font: fonts.bold,
    color: BRAND,
  })
  drawRight(page, "STATEMENT", right, cursor.y - 4, fonts.bold, 16, INK)
  cursor.y -= 30
  page.drawText(input.title, {
    x: MARGIN,
    y: cursor.y,
    size: 11,
    font: fonts.regular,
    color: MUTED,
  })
  drawRight(page, input.periodLabel, right, cursor.y, fonts.regular, 11, MUTED)
  cursor.y -= 18
  drawDivider(page, cursor.y)
  cursor.y -= 36

  // Summary figures.
  page.drawText("Total credits", {
    x: MARGIN,
    y: cursor.y,
    size: 10,
    font: fonts.regular,
    color: MUTED,
  })
  page.drawText("Ledger entries", {
    x: MARGIN + 230,
    y: cursor.y,
    size: 10,
    font: fonts.regular,
    color: MUTED,
  })
  cursor.y -= 26
  page.drawText(formatCredits(input.invoice.total_credits), {
    x: MARGIN,
    y: cursor.y,
    size: 24,
    font: fonts.bold,
    color: INK,
  })
  page.drawText(input.invoice.entries.toLocaleString("en-US"), {
    x: MARGIN + 230,
    y: cursor.y,
    size: 24,
    font: fonts.bold,
    color: INK,
  })
  cursor.y -= 26
  page.drawText(`Account ${input.invoice.account_id}`, {
    x: MARGIN,
    y: cursor.y,
    size: 8,
    font: fonts.regular,
    color: MUTED,
  })
  cursor.y -= 30
  drawDivider(page, cursor.y)
  cursor.y -= 24

  // Daily breakdown table.
  const colEntries = MARGIN + 360
  const colCredits = right
  page.drawText("Daily breakdown", {
    x: MARGIN,
    y: cursor.y,
    size: 12,
    font: fonts.bold,
    color: INK,
  })
  cursor.y -= 20
  page.drawText("Day", {
    x: MARGIN,
    y: cursor.y,
    size: 9,
    font: fonts.bold,
    color: MUTED,
  })
  drawRight(page, "Entries", colEntries, cursor.y, fonts.bold, 9, MUTED)
  drawRight(page, "Credits", colCredits, cursor.y, fonts.bold, 9, MUTED)
  cursor.y -= 8
  drawDivider(page, cursor.y)
  cursor.y -= 16

  const addPageIfNeeded = (): void => {
    if (cursor.y > MARGIN + 40) {
      return
    }
    page = doc.addPage([PAGE[0], PAGE[1]])
    cursor.y = PAGE[1] - MARGIN
  }

  if (input.days.length === 0) {
    page.drawText("No usage in this period.", {
      x: MARGIN,
      y: cursor.y,
      size: 10,
      font: fonts.regular,
      color: MUTED,
    })
    cursor.y -= 16
  }

  for (const day of input.days) {
    addPageIfNeeded()
    page.drawText(day.day, {
      x: MARGIN,
      y: cursor.y,
      size: 10,
      font: fonts.regular,
      color: INK,
    })
    drawRight(
      page,
      day.entry_count.toLocaleString("en-US"),
      colEntries,
      cursor.y,
      fonts.regular,
      10
    )
    drawRight(
      page,
      formatCredits(day.total_credits),
      colCredits,
      cursor.y,
      fonts.regular,
      10
    )
    cursor.y -= 16
  }

  // Footer note on the final page.
  page.drawText(
    "Summed deterministically from the meter ledger — enforced equals billed.",
    { x: MARGIN, y: MARGIN - 16, size: 8, font: fonts.regular, color: MUTED }
  )

  return doc.save()
}
