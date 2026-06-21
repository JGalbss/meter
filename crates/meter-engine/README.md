# meter-engine

The engine binary, `meter`. It connects to Postgres (money-truth) and ClickHouse (events + audit log,
ADR 0003/0004), applies both stores' migrations on boot, and serves the API from `meter-api` over HTTP
(`:8080`) and gRPC (`:50051`). This is the sole owner of money-truth — every credit movement in the
system passes through this process.

## Configuration

All configuration is environment variables.

| Variable | Default | Purpose |
|---|---|---|
| `METER_DATABASE_URL` | _(required)_ | Postgres connection for the ledger. |
| `METER_CLICKHOUSE_URL` | _(required)_ | ClickHouse connection for events + audit log. |
| `METER_LISTEN_ADDR` | `0.0.0.0:8080` | HTTP listen address. |
| `METER_GRPC_ADDR` | `0.0.0.0:50051` | gRPC listen address. |
| `METER_ROLES` | both | Surfaces this process serves: comma-separated `http`,`grpc`. Unset serves both; a deployment can run dedicated HTTP and gRPC replicas. |
| `METER_INGEST_MODE` | `exactly_once` | Event-ingest idempotency mode. `append` drops the cross-call dedup read for maximum throughput when ingest is made exactly-once upstream (ADR 0005). |
| `METER_CREDIT_VALUE` | `0.000001` | Cash value of one credit, USD, used to price usage into credits. |

An unknown role, or a `METER_ROLES` list that selects nothing, fails fast at startup rather than
silently serving nothing.

## Run it

```bash
METER_DATABASE_URL=postgres://meter:meter@localhost:5432/meter \
METER_CLICKHOUSE_URL=http://localhost:8123 \
  cargo run -p meter-engine
```

It runs the Postgres ledger migrations and the ClickHouse event migrations before binding, so a fresh
database comes up ready. `RUST_LOG` (via `tracing-subscriber`'s `EnvFilter`) controls log level;
default is `info`.

## Where it sits

A thin binary over `meter-api` (the surface), `meter-store-pg` and `meter-store-ch` (the stores), and
`meter-core` (the value types). The split where it serves dedicated HTTP- or gRPC-only roles is how the
engine scales horizontally.
