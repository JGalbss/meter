# ADR 0002 — Editable events, custom fields, and one-call run voiding

**Status:** Accepted (2026-06-19). **Extends** `ARCHITECTURE.md` §4 (ingest) and §6.4 (reversals).

## Context

Metronome treats usage events as effectively immutable and makes corrections painful — a top
complaint. Agent workloads need three things meter must make effortless: (1) events carrying arbitrary
custom fields; (2) the ability to **edit/correct** an event; (3) trivial **voiding of a failed agent
run** with clean reconciliation. The interfaces and SDKs must be the cleanest in the category.

## Decision

**Events are immutable facts; "edit" and "void" are first-class operations that append corrections —
never in-place mutation.** The append-only event store and ledger keep a perfect audit trail while the
API/SDK *feel* mutable.

### Event shape

```
event(
  id, org_id, idempotency_key,        -- client-owned uuidv7 → exactly-once
  event_time,                          -- business time (late data self-corrects)
  source, meter,                       -- which meter this counts toward
  subject { account_id, product_id, agent_id, user_id, … },
  quantity dimensions,                 -- tokens in/out/cache, actions, outcomes
  properties JSONB,                    -- arbitrary customer fields, schema-validated per meter
  run_id?,                             -- the agent run this belongs to
  status ∈ {recorded, amended, voided},
  supersedes_event_id?                 -- set on an amended version
)
```

`properties` is arbitrary customer JSON but **schema-validated at the write boundary** against the
meter's declared shape — expressive, not an opaque untyped bag (Lago's mistake).

### Runs — the unit of agent work

- A `run_id` groups the events and the reservation(s) of one agent run, and maps to a per-session lease
  account (`ARCHITECTURE.md` §5.4).
- **`void_run(run_id)`** is one call: it voids open holds and appends reversing ledger entries for
  everything the run settled, returning the credits. Idempotent. This is the "failed run" button
  Metronome lacks.

### Editing an event

- **`amend_event(event_id, patch)`** appends a new event version (`supersedes_event_id`); if the event
  was already priced/settled, it appends the **delta** as a ledger amendment (`reverses_entry_id`), so
  balances and invoices self-correct. The original stays in the audit trail.
- Edits to events on a **finalized** invoice roll forward as a credit-note (`ARCHITECTURE.md` §7.5) —
  never mutating the sealed invoice.

### Reconciliation

Because every correction is a ledger entry, reconciliation is just the existing invariants:
`SUM(all entries incl. reversals) == balance` and `enforced == billed`. Voids and amends are visible,
attributable, and reversible.

## Cleanest interfaces (SDK)

```
meter.event({ meter: "tokens", subject, input: 1200, output: 340, ...customFields })  // durable, idempotent
const run = meter.run({ account, product, agent })   // groups a run
await run.reserve(estimate)                            // gate before the LLM call
await run.settle(actualUsage)                          // priced posting
run.void()                                             // or auto-void on error/drop — failed run, gone
meter.amendEvent(eventId, patch)                       // clean "edit"
meter.voidRun(runId)                                   // clean run-level reversal
```

OTel-style auto-instrumentation emits the common events automatically; the explicit API covers custom
outcomes/artifacts and the run lifecycle. A failed run self-voids (RAII-style) so partial spend never
lingers.

## Consequences

- The event store keeps all versions; reads return the latest non-voided version.
- `run_id` becomes a core dimension across ingest, ledger leases, analytics, and invoicing.
- Slightly higher write volume (versions + reversals) — the deliberate price of a clean, auditable
  "editable events" UX.
