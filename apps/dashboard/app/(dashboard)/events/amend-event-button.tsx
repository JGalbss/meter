"use client"

import { type FormEvent, useState, useTransition } from "react"
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
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { amendEventAction } from "./actions"

export function AmendEventButton({
  eventId,
  properties,
}: {
  eventId: string
  properties: string
}) {
  const [open, setOpen] = useState(false)
  const [pending, startTransition] = useTransition()

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const data = new FormData(event.currentTarget)
    const json = String(data.get("properties") ?? "")
    startTransition(async () => {
      const result = await amendEventAction(eventId, json)
      if (!result.ok) {
        toast.error(result.error)
        return
      }
      toast.success("Event amended")
      setOpen(false)
    })
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button variant="outline" size="sm" />}>
        Amend
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Amend event</DialogTitle>
          <DialogDescription>
            Records a corrected version (append-only); the original is
            superseded, not overwritten. Edit the custom properties as JSON.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="properties">Properties (JSON)</Label>
            <Textarea
              id="properties"
              name="properties"
              defaultValue={properties}
              className="min-h-48 font-mono text-xs"
              spellCheck={false}
            />
          </div>
          <DialogFooter>
            <Button type="submit" disabled={pending}>
              Save amendment
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
