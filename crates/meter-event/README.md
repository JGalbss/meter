# meter-event

Usage events: immutable facts carrying arbitrary custom fields, grouped into runs, editable by amend and
voidable by run. It owns the `EventStore` trait — the seam every event backend implements — and an
in-memory reference. Editing is append-only; the audit trail is perfect, but reads behave as if the
event were edited in place.

## What's inside

| Item | What it is |
|---|---|
| `EventStore` | The async trait: `record` (idempotent), `record_batch` (the firehose path), `get`, `list_for_account`, `amend`, `void_run`. |
| `InMemoryEventStore` | The in-memory reference and conformance oracle. |
| `Event` / `EventStatus` | An event version and its status: `Recorded`, `Amended` (superseded), or `Voided`. Reads return the latest non-voided version. |
| `RecordEvent` / `AmendEvent` | The record and amend request types. |
| `idempotent_event_id` | Derives an event's id from `(org_id, idempotency_key)` so re-recording the same key yields the same id — no read-before-write to deduplicate. |

`amend` records a new version that supersedes the original (the original becomes `Amended`); `void_run`
marks every current event of a failed run `Voided`. Ingest is idempotent on `(org_id, idempotency_key)`,
which is what lets a content-addressed backend (a `ReplacingMergeTree` keyed on the id) collapse
duplicates without a read.

## The conformance suite

`crate::conformance`, behind the `conformance` feature, drives a backend through the trait only.
`run_all_scenarios` covers record, idempotency, amend supersession, batch ordering, void-run, and the
refused-after-void cases. **Every backend must pass it unchanged** — the in-memory reference and the
ClickHouse store (`meter-store-ch`, the system of record, ADR 0003) run the same suite.

## Where it sits

Builds on `meter-core`. `meter-store-ch` and `meter-api` build on it.

Edition 2021, `#![forbid(unsafe_code)]`.

```bash
cargo test -p meter-event
```
