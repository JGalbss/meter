import type { ComponentType } from "react"

export function EmptyState({
  icon: Icon,
  title,
  message,
}: {
  icon: ComponentType<{ size?: number; className?: string }>
  title: string
  message: string
}) {
  return (
    <div className="flex flex-col items-center justify-center rounded-lg border border-dashed p-12 text-center">
      <div className="mb-3 rounded-full bg-muted p-3 text-muted-foreground">
        <Icon size={20} />
      </div>
      {/* transitions.dev "texts reveal" (mount variant). */}
      <div className="t-stagger-reveal">
        <p className="t-stagger-line t-stagger-line--1 font-medium">{title}</p>
        <p className="t-stagger-line t-stagger-line--2 mt-1 max-w-sm text-sm text-muted-foreground">
          {message}
        </p>
      </div>
    </div>
  )
}
