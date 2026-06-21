# meter-ledger

The double-entry credit ledger: the `LedgerBackend` trait through which all money-truth flows, its
domain model, and an in-memory reference implementation. The ledger is append-only — balances are
derived from immutable entries, never edited in place.

## What's inside

| Item | What it is |
|---|---|
| `LedgerBackend` | The async trait every storage backend implements: `open_account`, `grant`, `refund`, `reserve`, `settle`, `charge`, `void`, `void_expired_holds`, `extend_hold`, `void_run`, `open_lease`, `close_lease`, `balance`, `entries`. |
| `InMemoryLedger` | The in-memory reference and conformance oracle. Correctness-only — O(n) in history, never a throughput target. |
| domain model | `LedgerAccount`, `LedgerEntry`, `Balance` (settled + held), `EntryType`, `CreditSource`, `LimitClass` (hard / soft), `AccountScope`, `SYSTEM_ACCOUNT`. |
| requests | `NewAccount`, `GrantRequest`, `ReserveRequest`, `SettleRequest`, `ChargeRequest`, `RefundRequest`, `LeaseRequest`, plus `ReserveOutcome` and `RunVoidSummary`. |

Reversals (refunds, voids, run-voids, amendment deltas) are new entries that reference the entry they
reverse, so "edit an event" and "void a failed run" stay clean without ever corrupting the audit trail.
Implementations must be safe under concurrency and make `grant`, `reserve`, `settle`, and `void`
idempotent on their keys.

## The conformance suite

`crate::conformance`, behind the `conformance` feature, is a reusable suite that drives a backend
through the trait only. `run_all_scenarios` runs every scenario; `check_against_model` and
`void_run_property` are proptest oracles. **Every backend must pass it unchanged** — the in-memory
reference and the Postgres backend (`meter-store-pg`) are tested against the same suite, so they behave
identically, including no overdraft under concurrent reserves.

## Where it sits

Builds on `meter-core`. `meter-enforcement`, `meter-store-pg`, and `meter-api` build on it.

Edition 2021, `#![forbid(unsafe_code)]`.

```bash
cargo test -p meter-ledger
```
