# meter-proto

The generated gRPC contract between the Rust engine and the TypeScript control plane. This crate is
pure codegen output: `build.rs` runs `tonic-build` over the `proto/` Buf module and emits the
`meter.v1` message types, client stubs, and server traits. The crate just re-exports them.

The source of truth is `proto/` at the workspace root — never hand-edit wire types here. The same
module also generates the control plane's TypeScript client (`ts-proto`/connect), so both sides stay in
lockstep and Buf's breaking-change check gates drift in CI.

## What's inside

Everything lives under `v1`:

- Messages and enums — `ReserveRequest`, `Credit`, `LimitClass`, `AccountScope`, and the rest of
  `meter.v1`. Decimal amounts cross the wire as strings (`Credit { amount: String }`), never floats.
- Service stubs — for each service, both a client and a server trait:
  `v1::ledger_service_client::LedgerServiceClient`, `v1::ingest_service_server::IngestServiceServer`,
  `v1::query_service_client::QueryServiceClient`, `v1::config_service_client::ConfigServiceClient`.

## Where it sits

Depends only on `prost` and `tonic` — no meter crates. `meter-api` implements the server side; the
control plane consumes the generated client.
