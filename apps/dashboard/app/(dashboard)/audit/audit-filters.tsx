"use client"

import { useRouter } from "next/navigation"
import { type FormEvent, useState } from "react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

const METHODS = [
  { value: "all", label: "All" },
  { value: "GET", label: "GET" },
  { value: "POST", label: "POST" },
  { value: "PUT", label: "PUT" },
  { value: "DELETE", label: "DELETE" },
] as const

const WINDOWS = [
  { value: "all", label: "All time" },
  { value: "24h", label: "Last 24h" },
  { value: "7d", label: "Last 7 days" },
  { value: "30d", label: "Last 30 days" },
] as const

export function AuditFilters({
  actor = "",
  method = "all",
  window = "all",
}: {
  actor?: string
  method?: string
  window?: string
}) {
  const router = useRouter()
  const [actorValue, setActorValue] = useState(actor)
  const [methodValue, setMethodValue] = useState(method)
  const [windowValue, setWindowValue] = useState(window)

  const apply = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const params = new URLSearchParams()
    const add = (key: string, value: string, skip: string) => {
      if (value.length > 0 && value !== skip) {
        params.set(key, value)
      }
    }
    add("actor", actorValue, "")
    add("method", methodValue, "all")
    add("window", windowValue, "all")
    const query = params.toString()
    if (query.length === 0) {
      router.push("/audit")
      return
    }
    router.push(`/audit?${query}`)
  }

  return (
    <form onSubmit={apply} className="mb-4 flex flex-wrap items-end gap-3">
      <div className="space-y-1">
        <Label htmlFor="actor">Actor</Label>
        <Input
          id="actor"
          value={actorValue}
          onChange={(event) => setActorValue(event.target.value)}
          placeholder="any"
          className="w-40"
        />
      </div>
      <div className="space-y-1">
        <Label htmlFor="method">Method</Label>
        <Select
          value={methodValue}
          onValueChange={(value) => setMethodValue(value ?? "all")}
        >
          <SelectTrigger id="method" className="w-28">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {METHODS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="space-y-1">
        <Label htmlFor="window">Window</Label>
        <Select
          value={windowValue}
          onValueChange={(value) => setWindowValue(value ?? "all")}
        >
          <SelectTrigger id="window" className="w-36">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {WINDOWS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <Button type="submit" size="sm">
        Apply
      </Button>
    </form>
  )
}
