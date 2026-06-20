# Control plane (TypeScript: Effect + Drizzle). Runs TypeScript directly via tsx and applies its
# Drizzle config migrations on boot. Computes no money — it calls the engine for that.

FROM node:24-slim AS runtime
WORKDIR /repo
RUN corepack enable

# Install only the control plane's production dependencies from the workspace lockfile.
COPY pnpm-workspace.yaml pnpm-lock.yaml package.json ./
COPY apps/control-plane/package.json apps/control-plane/package.json
RUN pnpm install --frozen-lockfile --filter @meter/control-plane --prod

# Source (TypeScript) + generated Drizzle migrations.
COPY apps/control-plane apps/control-plane

WORKDIR /repo/apps/control-plane
ENV NODE_ENV=production
ENV METER_CONTROL_PLANE_PORT=8090
EXPOSE 8090
CMD ["pnpm", "start"]
