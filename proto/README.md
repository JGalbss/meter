# `proto/` — the engine contract

The **source of truth** for the engine ⇄ control-plane wire contract (a [Buf](https://buf.build)
module, package `meter.v1`). Rust types + gRPC stubs are generated from these files by `meter-proto`
(`crates/meter-proto`, via `tonic-build`); the engine serves the services from `meter-api`'s `grpc`
module (`meter-engine` listens on `METER_GRPC_ADDR`, default `0.0.0.0:50051`).

> Per ADR 0001, money-truth lives in the engine. The control plane calls these services and never
> computes money itself.

## Services (`meter/v1/`)

| File | Service | Purpose |
| --- | --- | --- |
| `ledger.proto` | `LedgerService` | OpenAccount, Grant, Reserve, Settle, Void, ExtendHold, VoidExpiredHolds, VoidRun, Balance — the transactional money path |
| `ingest.proto` | `IngestService` | RecordEvent, RecordBatch, AmendEvent, VoidRun — the editable usage firehose |
| `query.proto` | `QueryService` | UsageByModel, UsageByField, UsageByDay, EventCount, Invoice — read-side analytics + billing |
| `config.proto` | `ConfigService` | PutRateCard, SetBudget — the control plane pushes pricing/budget rules to the engine |
| `common.proto` | — | shared `Money` / `Credit` value types |

## Conventions

- **Money and credits are exact decimals carried as strings** (`Money`, `Credit`), never floats — the
  wire must not introduce float error near money.
- **Ids are canonical UUID strings**; an empty string means "unset" for optional id fields
  (e.g. `parent_id`, `run_id`).
- **Timestamps are RFC3339 strings**; an empty `event_time` means "now".
- **Enums** mirror the engine domain exactly (`AccountScope`, `CreditSource`, `LimitClass`,
  `RateCardKind`) and use the `*_UNSPECIFIED = 0` zero value.
- `properties` on events is the customer's arbitrary JSON, carried as a string.

## Working with the contract

```sh
buf lint proto                    # STANDARD lint rules (enforced)
buf build proto -o /dev/null      # compile-check the module
buf breaking proto --against '.git#branch=main,subdir=proto'   # no breaking changes vs main
cargo build -p meter-proto        # regenerate the Rust types + stubs (tonic-build, needs protoc)
```

The Rust codegen runs automatically in `meter-proto`'s `build.rs`; there is no checked-in generated
code. Lint + breaking rules are configured in `buf.yaml`.
