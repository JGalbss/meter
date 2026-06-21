# meter-enforcement

Real-time budget enforcement on the request path. It prices an estimate, places a durable hold before
an agent call, then prices the actuals and posts them after — reserve before, settle after, void on
failure. This is where pricing and the ledger meet.

## What's inside

| Item | What it is |
|---|---|
| `EnforcementService<L>` | The service, generic over any `LedgerBackend` so the in-memory reference and Postgres are interchangeable. Holds the ledger and the cash value of one credit. |
| `reserve_usage` | Prices a worst-case `Usage` estimate against a `RateCard` into credits and places a hold. For a `LimitClass::Hard` limit, a `ReserveOutcome::Denied` means the call must not proceed. |
| `settle_usage` | Prices the actual usage and posts it, closing the reservation. Idempotent on the reservation id. Returns a `Settlement` (the ledger entry plus the `PricedUsage`). |
| `void` | Releases a reservation without charging it — a failed or abandoned run. |
| `ReservationPolicy` | Configurable reservation behaviour for the hold step. |

Hard limits are gated by the durable hold, so an out-of-budget agent is refused before it spends and can
never overdraft, even under concurrency — the no-overdraft guarantee comes from the ledger underneath.

## Where it sits

Composes three crates: `meter-pricing` to price usage into credits, `meter-ledger` to hold and post
them, and `meter-core` for the value types. It packages the reserve→settle flow as a reusable service;
the engine's HTTP/gRPC API (`meter-api`) currently implements the equivalent flow directly over the
ledger and pricing crates.

Edition 2021, `#![forbid(unsafe_code)]`.

```bash
cargo test -p meter-enforcement
```
