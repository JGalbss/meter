import type { MDXComponents } from "mdx/types";

// Hook required by @next/mdx (App Router). Map MDX elements to design-system-aligned
// components here as the docs grow; for now we pass through the defaults styled by globals.css.
export function useMDXComponents(components: MDXComponents): MDXComponents {
  return { ...components };
}
