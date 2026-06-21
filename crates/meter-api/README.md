# meter-api

The engine's API surface — one set of operations exposed over both HTTP (axum) and gRPC. It
deserializes requests into the domain types, calls the stores (`PgLedger` for money, `ChStore` for
events and the audit log), and serializes the results back. It computes no money itself; the ledger,
pricing, and enforcement crates own that.

The surface covers the full engine: accounts and balances, grants and credit notes, reserve / settle /
void enforcement, the metering loop (price → record → charge), model-priced reservation governance,
events with amend and `void_run`, the rate-card catalog, re-rate simulation, org analytics, and the
audit log.

## What's inside

- `router() -> axum::Router` — the HTTP router. Mounts `/v1/*`, `/health`, `/health/ready`, `/metrics`,
  and `/openapi.json`.
- `grpc::router(state) -> tonic::transport::server::Router` — the gRPC router serving the `meter.v1` Ledger, Ingest,
  Query, and Config services. `meter-engine` serves it alongside HTTP.
- `AppState` — shared handler state: the `PgLedger`, the `ChStore` event and audit stores, the credit's
  cash value (`Money`), and request metrics. Build it with `AppState::new(ledger, events, audit,
  credit_value)`.
- `openapi_document() -> utoipa::openapi::OpenApi` — the OpenAPI 3 document served at
  `GET /openapi.json`, exposed so tooling can emit it to a file for SDK codegen and drift-checking.
- `metrics::RequestMetrics` — the HTTP request and error counters surfaced at `GET /metrics`.

## Where it sits

Depends on the domain crates (`meter-core`, `meter-ledger`, `meter-event`, `meter-pricing`,
`meter-ratecards`), both stores (`meter-store-pg`, `meter-store-ch`), and the gRPC contract
(`meter-proto`). `meter-engine` is the binary that wires this into a running server.
