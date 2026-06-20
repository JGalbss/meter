#!/usr/bin/env bash
# Cross-stack e2e smoke: build + start the real stack (Postgres + ClickHouse + engine + control plane),
# wait for readiness, run the full money + config flow (deploy/e2e/flow.py), then tear down.
set -euo pipefail

cd "$(dirname "$0")/../.."
COMPOSE=(docker compose -f deploy/docker-compose.yml)

cleanup() { "${COMPOSE[@]}" down -v >/dev/null 2>&1 || true; }
trap cleanup EXIT

echo "== building + starting the stack =="
"${COMPOSE[@]}" up -d --build postgres clickhouse engine control-plane

wait_ready() {
  local url="$1" name="$2" i
  for i in $(seq 1 90); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      echo "  $name ready"
      return 0
    fi
    sleep 2
  done
  echo "  $name NOT ready after 180s — recent logs:"
  "${COMPOSE[@]}" logs --tail 40 "$name" || true
  return 1
}

echo "== waiting for readiness =="
wait_ready "http://localhost:8080/health/ready" engine
wait_ready "http://localhost:8090/health/ready" control-plane

echo "== running the cross-stack flow =="
python3 deploy/e2e/flow.py
