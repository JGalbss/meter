# Documentation site (apps/docs, Next.js + MDX, bun-managed). Builds a static export plus a Pagefind
# search index, then serves the plain HTML with nginx. Build context is the repo root (matches the
# engine/control-plane/dashboard Dockerfiles):
#   docker build -f deploy/docs.Dockerfile -t meter-docs .

FROM oven/bun:1.3.11 AS build
WORKDIR /app
# Install deps first for layer caching (the docs app has its own lockfile, outside the pnpm workspace).
COPY apps/docs/package.json apps/docs/bun.lock ./
RUN bun install --frozen-lockfile
COPY apps/docs/ ./
ENV NEXT_TELEMETRY_DISABLED=1
# `build` runs `next build` (static export to out/) then `pagefind --site out` to write the search index.
RUN bun run build

FROM nginx:1.27-alpine AS runtime
# Clean URLs: the export emits flat files (out/api/engine.html), so map /api/engine -> .html.
COPY deploy/docs.nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/out /usr/share/nginx/html
EXPOSE 80
