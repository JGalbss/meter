# meter — documentation site

The public docs for meter, built with Next.js + MDX and statically exported. Content lives in
`app/**/page.mdx` (Overview, Concepts, API reference, SDKs, Self-host); the shell and styling are in
`app/layout.tsx` and `app/globals.css`. Client-side search is built with Pagefind at build time.

```bash
pnpm --filter docs dev        # local dev server
pnpm --filter docs build      # sync OpenAPI → next build → pagefind index (the CI gate)
pnpm --filter docs typecheck
```

## Add a page

1. Create `app/<section>/page.mdx` and export its metadata at the top:

   ```mdx
   export const metadata = { title: "...", description: "..." };

   # Heading

   Markdown body…
   ```

2. Add the route to the `NAV` array in `components/sidebar.tsx`:

   ```ts
   { href: "/<section>", label: "..." }
   ```

No other registration is needed. Internal links use route paths (`/concepts`, `/self-host`). Diagrams
are Markdown tables or ASCII in a fenced block — no Mermaid.

## API reference

`app/api/engine/page.tsx` and `app/api/control-plane/page.tsx` are generated from the committed OpenAPI
contracts (synced into `lib/*-openapi.json` by `scripts/sync-openapi.mjs` during the build). Do not
hand-edit these pages — change the source spec instead.
