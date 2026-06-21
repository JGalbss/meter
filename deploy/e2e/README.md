# Cross-stack e2e smoke

A one-command smoke test of the whole stack. It brings up the real services (Postgres + ClickHouse +
engine + control plane) with `docker compose` and runs the full money and config flow against them —
the integration that per-layer tests (engine testcontainers; control plane against a stubbed engine)
don't cover.

```bash
bash deploy/e2e/smoke.sh
```

- `smoke.sh` — build + start the stack, wait on `/health/ready`, run the flow, tear down (`down -v`).
- `flow.py` — drives the **engine money flow through the Python SDK** (open → grant → meter usage →
  balance → invoice → budget → audit), so it doubles as the SDK-against-a-running-engine check, then
  exercises the control plane (create/list org, create a budget alert rule, evaluate → which calls the
  **real** engine for budget classification).

## Status

This harness has already caught two real cross-stack bugs that unit/per-layer tests missed:

1. **`protoc` missing from the engine build — fixed.** The engine depends on `meter-proto` (prost/tonic),
   whose build script needs the protobuf compiler. Neither `deploy/Dockerfile` nor the CI rust job
   installed it, so the engine image and the entire Rust CI failed to build. Both now install `protoc`.

2. **ClickHouse cross-container auth — fixed.** The engine's ClickHouse client (`meter-store-ch`) used
   `Client::default()` — user `default`, empty password, and no way to supply credentials. ClickHouse
   24.8 rejects a passwordless `default` user over a **remote** connection (it is localhost-only), so the
   engine crash-looped running its ClickHouse migrations on boot and never served. `ChStore` now layers
   credentials on via `with_credentials`/`with_database`, and the engine + CLI read them from the
   environment with `with_env_credentials`: `METER_CLICKHOUSE_USER`, `METER_CLICKHOUSE_PASSWORD`,
   `METER_CLICKHOUSE_DATABASE`. The compose ClickHouse service and the Helm chart set matching
   credentials, so the engine authenticates and serves.
