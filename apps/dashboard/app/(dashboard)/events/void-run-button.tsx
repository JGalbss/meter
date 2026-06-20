"use client"

import { useState, useTransition } from "react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { voidRunAction } from "./actions"

export function VoidRunButton({ runId }: { runId: string }) {
  const [open, setOpen] = useState(false)
  const [pending, startTransition] = useTransition()

  const confirm = () =>
    startTransition(async () => {
      const result = await voidRunAction(runId)
      if (!result.ok) {
        toast.error(result.error)
        return
      }
      toast.success(`Voided ${result.voided} event(s)`)
      setOpen(false)
    })

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button variant="outline" size="sm" />}>
        Void run
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Void this run?</DialogTitle>
          <DialogDescription>
            This voids every event for the run and reverses its ledger effects.
            The events drop out of usage and analytics. This cannot be undone
            from here.
          </DialogDescription>
        </DialogHeader>
        <p className="font-mono text-xs text-muted-foreground">{runId}</p>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => setOpen(false)}
            disabled={pending}
          >
            Cancel
          </Button>
          <Button variant="destructive" onClick={confirm} disabled={pending}>
            Void run
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
