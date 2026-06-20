# Dashboard (Next.js, bun-managed). Builds a standalone server bundle and runs it on a slim runtime.
# Build context is the repo root (matches the engine/control-plane Dockerfiles).

FROM oven/bun:1.3.11 AS build
WORKDIR /app
# Install deps first for layer caching (the dashboard is outside the pnpm workspace; its own lockfile).
COPY apps/dashboard/package.json apps/dashboard/bun.lock ./
RUN bun install --frozen-lockfile
COPY apps/dashboard/ ./
ENV NEXT_TELEMETRY_DISABLED=1
RUN bun run build

FROM oven/bun:1.3.11-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000
ENV HOSTNAME=0.0.0.0
# The standalone bundle is self-contained; static assets + public are copied alongside it.
COPY --from=build /app/.next/standalone ./
COPY --from=build /app/.next/static ./.next/static
COPY --from=build /app/public ./public
EXPOSE 3000
CMD ["bun", "server.js"]
