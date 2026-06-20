# meter — documentation site

The public docs for meter, built with **Next.js + MDX**. Content lives in `app/**/page.mdx`
(Overview, Concepts, API reference, SDKs, Self-host); the shell and styling are in `app/layout.tsx`
and `app/globals.css`.

```bash
bun install
bun run dev      # local dev server
bun run build    # production build (static-prerendered)
bun run typecheck
```

CI typechecks and builds this site. Keep it current as the engine/control-plane surface evolves.
