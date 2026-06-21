import type { NextConfig } from "next"

// `standalone` emits a self-contained server bundle for a slim production image. The dashboard is
// bun-managed (its own lockfile/node_modules, outside the pnpm workspace), so pin the file-tracing
// root to this directory — otherwise Next traces from the monorepo root and nests the output.
const nextConfig: NextConfig = {
  output: "standalone",
  outputFileTracingRoot: import.meta.dirname,
  experimental: {
    // Keep the client-side router cache warm so re-visiting a tab is instant instead of re-streaming a
    // fresh server render (the "skeleton on every tab" complaint). Dynamic pages stay fresh enough for
    // an operator console while eliminating the flash on back-and-forth navigation.
    staleTimes: {
      dynamic: 180,
      static: 300,
    },
  },
}

export default nextConfig
