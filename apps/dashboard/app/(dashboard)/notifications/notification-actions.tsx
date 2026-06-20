"use client"

import { useTransition } from "react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { ackAction, type ActionResult, markReadAction } from "./actions"

function isUnread(status: string): boolean {
  return status === "unread"
}

function isAcked(status: string): boolean {
  return status === "acked"
}

export function NotificationActions({
  id,
  status,
}: {
  id: string
  status: string
}) {
  const [pending, startTransition] = useTransition()

  const run = (action: () => Promise<ActionResult>, success: string) =>
    startTransition(async () => {
      const result = await action()
      if (!result.ok) {
        toast.error(result.error)
        return
      }
      toast.success(success)
    })

  return (
    <div className="flex justify-end gap-2">
      <Button
        variant="outline"
        size="sm"
        disabled={pending || !isUnread(status)}
        onClick={() => run(() => markReadAction(id), "Marked read")}
      >
        Mark read
      </Button>
      <Button
        variant="outline"
        size="sm"
        disabled={pending || isAcked(status)}
        onClick={() => run(() => ackAction(id), "Acknowledged")}
      >
        Ack
      </Button>
    </div>
  )
}
