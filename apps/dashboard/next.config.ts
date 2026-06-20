import type { NextConfig } from "next"

// `standalone` emits a self-contained server bundle for a slim production image. The dashboard is
// bun-managed (its own lockfile/node_modules, outside the pnpm workspace), so pin the file-tracing
// root to this directory — otherwise Next traces from the monorepo root and nests the output.
const nextConfig: NextConfig = {
  output: "standalone",
  outputFileTracingRoot: import.meta.dirname,
}

export default nextConfig
