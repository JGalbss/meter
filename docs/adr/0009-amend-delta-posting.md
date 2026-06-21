# ADR 0009 — Event amendment → ledger delta posting

**Status:** Accepted & implemented (2026-06-21). **Implements** ADR 0002 §"Editing an event" and
`ARCHITECTURE.md` §6.4 (reversals). Both halves shipped: the remainder-based reversal foundation (commit
faf6a59) and the amend delta-posting (commit d337f22) — `POST /v1/usage/{id}/amend`, reverse-and-recharge
via the shared `reverse_charge` primitive, idempotent, e2e-tested (up/down chain, idempotent replay,
void-restores, refused-after-void).

## Foundation shipped (2026-06-21, commit faf6a59)

Voiding a failed run now reverses its **direct** `/v1/usage` charges, not just reservations: charges
carry `run_id` (a nullable `ledger_entries.run_id` column), and `void_run` reverses each charge's
**unreversed remainder** — its original magnitude minus every refund that already references it via
`reverses_entry_id`. This is idempotent on re-void and never double-refunds a charge already corrected
by a linked credit-note (an adversarial review surfaced and we closed that conservation hole; an
*unlinked* credit-note is independent and does not offset the charge). Proven on both ledger backends
via the shared conformance suite + an HTTP e2e. This remainder-based reversal is the primitive the amend
delta-posting reuses.

## Refinement: chain-safe amend (supersedes the per-charge delta sketch below)

A naive "post the signed delta, linking delta-downs to the original charge" breaks under **chained
amends with mixed up/down deltas** (a delta-down after a delta-up can drive a charge's remainder
negative, and void then over- or under-refunds). The correct, chain-safe model is **reverse-and-
recharge by remainder**: an amend (1) reverses the event's *current* charge by its unreversed remainder
(a linked refund — the same primitive `void_run` uses), then (2) posts a fresh charge for the re-priced
new amount (engine-computed, honouring the original event's immutable `burnable` flag), and (3) records
the amended event version pointing at the new charge. Net account effect = new − old, every step is
idempotent on the amend key, and void/manual-refund interactions stay conservation-exact because every
reversal nets against an entry's remainder. The original `/v1/usage` charge records its ledger entry id
on the event so the amend can target it.

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
