"use client"

import { DownloadSimple } from "@phosphor-icons/react"

import { Button } from "@/components/ui/button"
import { auditToCsv } from "@/lib/meter/csv"
import type { AuditEntry } from "@/lib/meter/types"

export function ExportAuditButton({
  entries,
}: {
  entries: readonly AuditEntry[]
}) {
  const download = () => {
    const blob = new Blob([auditToCsv(entries)], {
      type: "text/csv;charset=utf-8",
    })
    const url = URL.createObjectURL(blob)
    const link = document.createElement("a")
    link.href = url
    link.download = "audit-log.csv"
    link.click()
    URL.revokeObjectURL(url)
  }

  return (
    <Button
      variant="outline"
      size="sm"
      onClick={download}
      disabled={entries.length === 0}
    >
      <DownloadSimple size={16} />
      Export CSV
    </Button>
  )
}
