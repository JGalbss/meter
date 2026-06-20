"use client"

import { useTransition } from "react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { revokeApiKeyAction } from "./actions"

export function RevokeApiKeyButton({ id }: { id: string }) {
  const [pending, startTransition] = useTransition()

  const revoke = () =>
    startTransition(async () => {
      const result = await revokeApiKeyAction(id)
      if (!result.ok) {
        toast.error(result.error)
        return
      }
      toast.success("API key revoked")
    })

  return (
    <Button variant="outline" size="sm" disabled={pending} onClick={revoke}>
      Revoke
    </Button>
  )
}
