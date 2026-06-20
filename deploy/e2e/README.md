# Cross-stack e2e smoke

Brings up the real stack (Postgres + ClickHouse + engine + control plane) with `docker compose` and
runs the full money + config flow against it — the integration that per-layer tests (engine
testcontainers; control-plane against a stubbed engine) don't cover.

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

2. **ClickHouse cross-container auth — open, engine-side.** The engine's ClickHouse client
   (`meter-store-ch`) connects with `Client::default()` — user `default`, empty password, and **no way
   to supply credentials** (`with_url` does not parse them; `with_user`/`with_password` are never
   called). ClickHouse 24.8 rejects a passwordless `default` user over a **remote** connection (it is
   localhost-only), so the engine crash-loops running its ClickHouse migrations on boot and never serves.
   Fixing this properly requires the engine to read ClickHouse credentials from the environment and call
   `with_user`/`with_password`, paired with matching credentials on the ClickHouse service in compose
   (and Helm). Until that lands, `smoke.sh` fails at the readiness wait — which is the harness correctly
   reporting a real, open integration gap, not a flaky test.
