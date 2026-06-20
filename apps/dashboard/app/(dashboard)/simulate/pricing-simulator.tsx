"use client"

import { type FormEvent, useState, useTransition } from "react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { RateCardEntry, SimulateResult } from "@/lib/meter/types"
import { simulateAction } from "./actions"

const DIMENSIONS = [
  { name: "input_uncached", label: "Input (uncached)" },
  { name: "cache_read", label: "Cache read" },
  { name: "cache_write", label: "Cache write" },
  { name: "output", label: "Output" },
  { name: "reasoning", label: "Reasoning" },
] as const

function toCount(value: FormDataEntryValue | null): number {
  const parsed = Number(value ?? 0)
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return 0
  }
  return Math.floor(parsed)
}

function isZeroDelta(delta: string): boolean {
  return /^-?0(\.0+)?$/.test(delta)
}

function isCheaper(delta: string): boolean {
  return delta.startsWith("-")
}

function deltaLabel(delta: string): string {
  if (isZeroDelta(delta)) {
    return "Same cost on both models."
  }
  if (isCheaper(delta)) {
    return "The proposed model is cheaper for this usage."
  }
  return "The proposed model is more expensive for this usage."
}

export function PricingSimulator({
  models,
}: {
  models: readonly RateCardEntry[]
}) {
  const firstModel = models[0]?.model_id ?? ""
  const secondModel = models[1]?.model_id ?? firstModel
  const [current, setCurrent] = useState(firstModel)
  const [proposed, setProposed] = useState(secondModel)
  const [result, setResult] = useState<SimulateResult | null>(null)
  const [pending, startTransition] = useTransition()

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const data = new FormData(event.currentTarget)
    const usage = {
      input_uncached: toCount(data.get("input_uncached")),
      cache_read: toCount(data.get("cache_read")),
      cache_write: toCount(data.get("cache_write")),
      output: toCount(data.get("output")),
      reasoning: toCount(data.get("reasoning")),
    }
    startTransition(async () => {
      const outcome = await simulateAction({
        current_model: current,
        proposed_model: proposed,
        events: [usage],
      })
      if (!outcome.ok) {
        toast.error(outcome.error)
        return
      }
      setResult(outcome.summary)
    })
  }

  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <Card>
        <CardContent className="pt-6">
          <form onSubmit={submit} className="space-y-4">
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="current">Current model</Label>
                <Select
                  value={current}
                  onValueChange={(value) => setCurrent(value ?? "")}
                >
                  <SelectTrigger id="current">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {models.map((model) => (
                      <SelectItem key={model.model_id} value={model.model_id}>
                        {model.model_id}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="proposed">Proposed model</Label>
                <Select
                  value={proposed}
                  onValueChange={(value) => setProposed(value ?? "")}
                >
                  <SelectTrigger id="proposed">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {models.map((model) => (
                      <SelectItem key={model.model_id} value={model.model_id}>
                        {model.model_id}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              {DIMENSIONS.map((dimension) => (
                <div key={dimension.name} className="space-y-2">
                  <Label htmlFor={dimension.name}>{dimension.label}</Label>
                  <Input
                    id={dimension.name}
                    name={dimension.name}
                    type="number"
                    min="0"
                    defaultValue="0"
                  />
                </div>
              ))}
            </div>
            <Button type="submit" disabled={pending}>
              Simulate
            </Button>
          </form>
        </CardContent>
      </Card>

      {result !== null && (
        <Card>
          <CardContent className="space-y-4 pt-6">
            <p className="text-sm font-medium text-muted-foreground">
              {result.event_count} event(s) re-rated
            </p>
            <dl className="space-y-3">
              <div className="flex items-baseline justify-between">
                <dt className="text-sm text-muted-foreground">
                  {result.current_model}
                </dt>
                <dd className="font-mono text-sm">
                  {result.credits_current} credits
                </dd>
              </div>
              <div className="flex items-baseline justify-between">
                <dt className="text-sm text-muted-foreground">
                  {result.proposed_model}
                </dt>
                <dd className="font-mono text-sm">
                  {result.credits_proposed} credits
                </dd>
              </div>
              <div className="flex items-baseline justify-between border-t pt-3">
                <dt className="text-sm font-medium">Delta</dt>
                <dd className="font-mono text-sm font-medium">
                  {result.credit_delta} credits
                </dd>
              </div>
            </dl>
            <p className="text-sm text-muted-foreground">
              {deltaLabel(result.credit_delta)}
            </p>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
