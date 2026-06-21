# ADR 0009 — Event amendment → ledger delta posting

**Status:** Proposed (2026-06-21). **Implements** ADR 0002 §"Editing an event" and `ARCHITECTURE.md`
§6.4 (reversals). Needs acceptance before implementation — it changes money-truth behaviour.

## Context

ADR 0002 mandates that amending an already-priced event appends the **delta** to the ledger so balances
and invoices self-correct, never an in-place mutation. The engine implements the event half — `amend`
appends a superseding version, and is now idempotent on an optional key (rollup-safe under retries) —
but **not the ledger delta**. Two things block a safe implementation:

1. **The generic amend can't be trusted with money.** `POST /v1/events/{id}/amend` replaces arbitrary
   `properties`, including `credits`. Money-truth lives only in the engine (ADR 0001): a caller must
   never be able to set the charged amount, so a delta can't be derived from caller-supplied `credits`.
2. **No decision on which events qualify or how the delta links back.** Raw `/v1/events` rows that were
   never charged must not suddenly post a charge; refunds should reference the original entry.

## Decision (proposed)

Add a **usage-aware amendment** that re-prices in the engine and posts the engine-computed delta. The
generic property-only amend stays as-is (no ledger effect); money corrections go through the new path.

### Endpoint

```
POST /v1/usage/{event_id}/amend
  { usage: UsageDimensions,        // corrected token counts
    model?, rate_card_id?,         // default to the original event's
    idempotency_key? }             // makes the amend + its delta idempotent
```

### Behaviour

1. **Qualify.** Load the original event. It must be engine-priced (carries `priced_via`, ADR 0001) and
   not voided. A raw event (no `priced_via`) is rejected — there is no original charge to adjust.
2. **Re-price in the engine.** Price the corrected `usage` against the resolved card (the original's
   card/version by default), honouring the original event's `burnable` flag — non-burnable stays
   `burned = 0`. The caller never supplies credits.
3. **Append the version.** Amend the event (the idempotent keyed path) with the engine-computed
   `credits`/`priced_credits`/provenance, exactly like `/v1/usage` records them.
4. **Post the delta.** `delta = new_burned − old_burned` (the original event's recorded `credits` is the
   ground truth for what was charged):
   - `delta > 0` → `charge` the difference.
   - `delta < 0` → `refund` the difference, with `reverses_entry_id` pointing at the original charge
     (ADR 0002's linkage; the field already exists on `RefundRequest`).
   - `delta == 0` → no posting.
   Idempotent: the posting's idempotency key is derived from the amended event id (stable for a keyed
   amend), so a retry never double-posts.

### Event ↔ ledger linkage

`/v1/usage` already records `credits` (= burned) on the event; it will additionally record the **charge
entry id** in the event's properties (additive provenance, no ledger schema change). The amendment reads
it to set `reverses_entry_id` on a refund, closing the audit loop (charge → its amendment).

## Consequences

- Money corrections are engine-computed and idempotent; callers cannot spoof a charge.
- No ledger schema change — reuses `charge` / `refund(reverses_entry_id)` and event-property provenance.
- The generic event amend remains non-financial; only the usage-amend path moves money, which keeps the
  blast radius small and the audit trail clean (`SUM(entries incl. reversals) == balance` still holds).
- Reservation/settle-sourced usage is out of scope here (those corrections go through re-settle); this
  path covers the direct-charge `/v1/usage` model.

## Alternatives rejected

- **Re-pricing inside the generic `/v1/events/{id}/amend`** — overloads a general edit with money
  behaviour and still has to ignore caller `credits`; a dedicated usage path is clearer and safer.
- **Trusting caller-supplied `credits` in the delta** — violates ADR 0001 (money-truth only in the
  engine); a caller could mint or erase charges.
