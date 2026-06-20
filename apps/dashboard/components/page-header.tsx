import type { ReactNode } from "react";

export function PageHeader({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="mb-6 flex items-start justify-between gap-4">
      {/* transitions.dev "texts reveal" (mount variant): staggered blurred rise on first paint. */}
      <div className="t-stagger-reveal space-y-1">
        <h1 className="t-stagger-line t-stagger-line--1 font-heading text-2xl font-semibold tracking-tight">
          {title}
        </h1>
        {description && (
          <p className="t-stagger-line t-stagger-line--2 text-sm text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {action}
    </div>
  );
}
