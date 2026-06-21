"use client"

import { Check, Copy } from "@phosphor-icons/react"
import { useRef, useState } from "react"

import { Button } from "@/components/ui/button"

type CopyState = "a" | "b"

// Copy-to-clipboard with the transitions.dev icon-swap: the copy glyph cross-fades to a check on
// success, then reverts. Used for the API-key token and the onboarding key reveal.
export function CopyButton({
  value,
  label,
  className,
}: {
  value: string
  label?: string
  className?: string
}) {
  const [state, setState] = useState<CopyState>("a")
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const copy = async () => {
    await navigator.clipboard.writeText(value)
    setState("b")
    if (timer.current !== null) {
      clearTimeout(timer.current)
    }
    timer.current = setTimeout(() => setState("a"), 1600)
  }

  return (
    <Button
      type="button"
      variant="outline"
      size={label === undefined ? "icon-sm" : "sm"}
      className={className}
      onClick={copy}
      aria-label={label ?? "Copy"}
    >
      <span className="t-icon-swap" data-state={state}>
        <Copy className="t-icon" data-icon="a" size={14} />
        <Check className="t-icon" data-icon="b" size={14} weight="bold" />
      </span>
      {label !== undefined && <span>{label}</span>}
    </Button>
  )
}
