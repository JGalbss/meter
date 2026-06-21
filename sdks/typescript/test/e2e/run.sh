#!/usr/bin/env bash
# Stand up the engine stack (Postgres + ClickHouse + the engine) and run the SDK e2e test against it.
# Verifies the SDK ⇄ engine HTTP contract end to end with real infrastructure.
#
#   ./test/e2e/run.sh
#
# Requires Docker and a built workspace. Tears the engine down on exit; leaves the dev containers up
# (re-run is fast). Pass KEEP_ENGINE=1 to leave the engine running for manual poking.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
sdk_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
compose="$repo_root/deploy/dev/docker-compose.yml"

echo "==> bringing up Postgres + ClickHouse"
docker compose -f "$compose" up -d postgres clickhouse

echo "==> waiting for Postgres + ClickHouse health"
for _ in $(seq 1 60); do
  pg=$(docker inspect -f '{{.State.Health.Status}}' meter-dev-postgres-1 2>/dev/null || echo starting)
  ch=$(docker inspect -f '{{.State.Health.Status}}' meter-dev-clickhouse-1 2>/dev/null || echo starting)
  [ "$pg" = healthy ] && [ "$ch" = healthy ] && break
  sleep 2
done

echo "==> starting the engine"
export METER_DATABASE_URL="postgres://meter:meter@localhost:5432/meter"
export METER_CLICKHOUSE_URL="http://meter:meter@localhost:8123"
export METER_LISTEN_ADDR="127.0.0.1:8080"
(cd "$repo_root" && cargo run --quiet -p meter-engine) &
engine_pid=$!
cleanup() {
  [ "${KEEP_ENGINE:-0}" = 1 ] || kill "$engine_pid" 2>/dev/null || true
}
trap cleanup EXIT

echo "==> waiting for engine /health"
for _ in $(seq 1 60); do
  curl -sf http://127.0.0.1:8080/health >/dev/null 2>&1 && break
  sleep 1
done
curl -sf http://127.0.0.1:8080/health >/dev/null

echo "==> running the SDK e2e suite"
cd "$sdk_dir"
METER_E2E_BASE_URL="http://127.0.0.1:8080" pnpm test e2e
