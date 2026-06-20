import { Badge } from "@/components/ui/badge"

export type BadgeVariant = "default" | "secondary" | "destructive" | "outline"

/** Render a string value as a badge whose variant is chosen from a lookup map (default: outline). */
export function ValueBadge({
  value,
  variants,
}: {
  value: string
  variants: Record<string, BadgeVariant>
}) {
  return <Badge variant={variants[value] ?? "outline"}>{value}</Badge>
}
