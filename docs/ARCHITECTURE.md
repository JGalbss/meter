# meter — Architecture Decision Record

> Status: **Locked v1 baseline.** Supersedes the five-store / two-language proposal.
> Authority: this ADR is reconciled against `docs/DECISIONS.md` (10 locked decisions) and
> `docs/VISION.md`. Where the proposed architecture conflicted with a locked decision, the locked
> decision wins unless this ADR explicitly reopens it with stated reasoning. Last revised: 2026-06-19.

---

## 0. How this ADR was produced (and the most important correction)

The proposed architecture was reviewed by three adversarial skeptics (performance/scalability,
operations/self-host/simplicity, and financial-correctness/ledger). The single most consequential
finding is **procedural, not technical**:

> The proposal silently reversed four locked decisions in `docs/DECISIONS.md`: it added a second
> backend language (Effect-TS), promoted the explicitly-deferred TigerBeetle to *the* authoritative
> ledger, demoted Postgres (the locked system of record) to a "projection", and mandated
> Redpanda + Redis in the default footprint — all stamped `confidence: high`.

The repo is not a blank slate. `Cargo.toml` already pins `axum 0.7` + `sqlx 0.8` (Postgres);
`crates/meter-core` already implements `Money` and `Credit` as `rust_decimal::Decimal` (not 128-bit
integers); `CLAUDE.md` lists `meter-ledger` as a *"double-entry ledger engine over Postgres"* and
`meter-api` as *"axum HTTP API + OpenAPI generation"*; the dev `docker-compose.yml` is **Postgres +
ClickHouse only**; the README promises *"Minimal footprint: the `meter` binary + PostgreSQL."*
The founder's global `~/.claude/CLAUDE.md` Effect-TS preference is **explicitly scoped to TypeScript
(SDK + dashboard) by the repo's own `CLAUDE.md`** — "the backend is Rust and has its own rules."

Therefore this ADR **honors the locked decisions** and treats the proposal's exotic stores as what
DECISIONS.md already called them: optional, opt-in scale-out backends. This simultaneously resolves the
ops critic's "five stateful systems + two languages is more complex than Metronome" verdict **and** the
financial critic's #1 critique ("two authoritative ledgers reconciled by hope") — because with Postgres
as the single source of truth for money by default, there is exactly one money ledger.

The technical substance of all three critiques is then addressed below, on top of the locked baseline.

### Resolving the critics' disagreement (more systems vs. fewer)

The performance critic wants more systems sooner (TigerBeetle, Redis, sharded ClickHouse, lease tiers).
The ops critic wants the smallest possible default. They are optimizing for different users. The call,
consistent with VISION §6 ("smallest default footprint") and DECISIONS #1/#2:

- **The simplest correct deployment is the design center.** Default = **one Rust binary + Postgres**
  (+ optional ClickHouse). This is the architecture for ~95% of self-hosters and the entire single-tenant
  story, and it is *fully correct* — not a degraded "Lite" fork.
- **Every system the perf critic wants is real and kept — as an opt-in backend behind a stable trait,
  activated by a documented, measured trigger** (stated as numbers in §9). The hosted tier and large
  self-hosters get TigerBeetle, Redis, Redpanda, and ClickHouse sharding *when their numbers justify it*,
  not before.
- **Crucially, the money-correctness code path does not fork.** There is **one ledger code path**
  (Postgres double-entry) that ships everywhere. TigerBeetle is offered only as a pluggable *balance/holds
  accelerator* that must pass the exact same conformance + bill-equivalence suite. This kills the
  "two correctness regimes wearing one trait" trap both the perf and ops critics flagged about "Lite vs Full."

---

## 1. Vision

meter turns raw agent and API usage into **credits, budgets, and invoices**, backed by an **immutable,
double-entry ledger that is the single auditable source of truth**. It ingests usage at high throughput,
enforces budgets and credit limits in real time on the request hot path with **no overdraft**, and keeps
its concepts deliberately small. It is built **agent-first** (an agent's runtime can meter and govern
itself) and **human-usable** (configure, price, budget, invoice with confidence). It is **open source
(AGPL-3.0) and self-hostable** in a customer's VPC — single-tenant or multi-tenant — with a managed cloud.

The wedge competitors don't ship: **real-time, no-overdraft reserve→settle enforcement on the LLM hot
path**, a **conceptual model cleaner than Metronome**, a **performance ceiling far above CRUD-billing
tools**, and a **batteries-included, always-current model rate-card catalog**.

---

## 2. The conceptual model (cleaner than Metronome)

Metronome's #1 complaint is ~9 overlapping nouns (event, billable-metric, product, rate, rate-card,
rate-card-alias, contract, commit, credit). meter collapses the top level to **six nouns**:

| Noun | What it is | Replaces in Metronome |
|---|---|---|
| **Event** | One idempotent, immutable usage fact (tokens, an action, an outcome). | event |
| **Meter** | A named way to aggregate events into a billable/segmentable quantity (count/sum/max/unique/latest/weighted_sum) + its presentation unit. | billable-metric (+ product's display role) |
| **RateCard** | A *versioned, dimensional, priced* mapping from meter quantities → credits. One card, many priced dimensions. | product + rate + rate-card + rate-card-alias |
| **Account** | A node in the org→team→user hierarchy **and** the carrier of contract terms. | contract + customer hierarchy |
| **Grant / Budget** | Credit pools and time-boxed limits (two distinct ideas, see §6). | commit + credit + (budget had no first-class noun) |
| **Invoice** | A deterministic statement for a billing period. | invoice |

**Why this is genuinely simpler — measured, not asserted.** The ops critic correctly warned that "fewer
nouns" can be illusory if per-noun configuration sprawls. We adopt a hard product gate: the simplicity
claim is validated by **time-to-first-correct-invoice** and a **step-count walkthrough** ("price one
Anthropic model with cache tiers") benchmarked against Metronome — *not* by noun count. Concretely we
take three of the ops critic's recommendations into the model:

1. **One card type, not two parallel schemas.** The provider-cost catalog and the customer rate card
   share **one** `rate_card` shape distinguished by a `kind` (`provider_cost` | `customer`) and a
   first-class `margin`. Users learn one editor, not two.
2. **Templates over blank matrices.** The common case is "pick a model, set margin" — rate cards are
   instantiated from **catalog-backed templates** pre-filled per provider/model, so a user never faces an
   empty 6-axis grid for the common path.
3. **Defer indirection.** `pricing_units` / `applied_pricing_units` multi-currency-conversion indirection
   and tag-based override routing are **out of v1** unless a concrete user story needs them; v1 ships a
   single `customer_rate_override` table and single-currency-per-org (see §11 on FX).

**Four independent time axes** (a founder hard requirement, kept exactly): rate-card validity windows,
periodic budgets, credit-block expiry, and invoice/billing periods **never share a period definition**.
A weekly "400 credits/week/product" budget is a `budget` + `budget_cycle`; a biannual prepaid grant is a
`credit_block`; they are reconciled through the *one* ledger, not a parallel counter (see §6).

---

## 3. System architecture

### 3.1 One backend, one binary, one money type (Decision #1, #4 — honored)

There is **one Rust backend** (control plane + data plane in one Cargo workspace) that ships as **one
self-host binary**, `meter-server`, composed of independently-runnable services. There is **no second
backend language** and therefore **no internal Protobuf/Buf contract spine** — the cross-language money
boundary the proposal introduced (and the drift risk it carried) does not exist. TypeScript is
**client-side only** (dashboard + SDK), and its types are **codegen'd from the backend's versioned
OpenAPI** (Decision #6), which keeps the all-Rust decision reversible.

> *Reopening note for the founder:* if Effect-TS for the control plane is genuinely wanted for product
> velocity, that is a legitimate RFC to **explicitly reopen Decision #1** — it must be a deliberate,
> recorded reversal with the contributor-funnel and money-drift costs weighed, not smuggled into an
> architecture doc. Recommended default: **stay all-Rust** (`axum` is entirely adequate for CRUD; the
> contributor funnel widens by not requiring both idiomatic Rust *and* Effect's Tag/Layer/typed-error model).

**Crate layout** (matches existing `CLAUDE.md`):
`meter-core` (Money/Credit/ids/errors), `meter-pricing`, `meter-ledger` (double-entry over the ledger
trait), `meter-enforcement` (reserve/settle), `meter-ingest`, `meter-store-pg`, `meter-store-ch`,
`meter-api` (axum + OpenAPI), `meter-server` (the binary), `meter-ratecards` (catalog), `meter-cli`.

**Independently deployable services** (one binary, role-selected by flag/config — `meter-server --role …`):
public API, ledger/enforcement, ingest worker, scheduler, webhooks worker, invoice renderer, catalog
scraper. Single-binary self-host runs them in-process; the hosted tier runs them as separate deployments.

### 3.2 Storage engines and why each earns its keep

The default is **two stores**. Three more are **opt-in scale-out backends**, each behind a stable Rust
trait, each gated by a measured trigger (§9).

| Store | Tier | Role | Why / why-not |
|---|---|---|---|
| **PostgreSQL** | **Default (always)** | **System of record for money & config**: hierarchy, RBAC, rate cards, grants, budgets, invoices, **and the authoritative double-entry ledger** (immutable entries, derived balances). Also the default durable ingest queue (outbox). | Decision #2/#7. Credit-transaction volume ≪ raw-event volume; a tuned single primary + replicas scales very far. Hot-row contention (real: ~160–430 TPS on one contended row) is solved by **balance sharding / per-session leasing** (§5), not by adding a second money store. |
| **ClickHouse** | **First optional add-on** | Raw immutable usage-event store + minute/day rollups + analytics/dashboards + deterministic re-rating evidence. **Analytics & dispute evidence — not a billing authority.** | Decision #3. Best columnar scan/ops. *Not* the ledger (merge-time dedup is eventual). Optional for small self-host (synchronous Postgres event path covers low volume). |
| **TigerBeetle** | **Opt-in ledger accelerator** | Pluggable balances/holds backend for tenants whose ledger write-TPS exceeds a tuned Postgres primary. Native two-phase pending transfers, `debits_must_not_exceed_credits`, DB-managed hold timeouts. | DECISIONS.md deferred it explicitly. Verified 2026: it **funnels all writes through one core to *eliminate* hot-account contention** (refuting the perf critic's "hot org pool" worry) and **auto-shrinks batches under light load for latency**. But it is a separate stateful system (leader-based; more replicas = reliability, not throughput) with an immutable integer schema — so it is an *accelerator*, never the default. |
| **Redpanda** (Kafka API) | **Opt-in ingest log** | Durable partitioned commit log for ingest at scale: buffering, backpressure, replay. | DECISIONS.md deferred: "make Kafka/Redpanda opt-in, smaller default footprint." Single C++ binary (good self-host story) but still a partitioned/replicated system to size & back up. Default uses a **Postgres outbox** consumed by the same worker. |
| **Redis** | **Opt-in hot-path accelerator** | Sub-ms Lua check-and-decrement gate for tenants who measurably need <few-ms enforcement decisions. | DECISIONS.md deferred. Removed from the default footprint (the proposal wrongly put it in "Lite"). Default enforcement is Postgres advisory-lock/leased decision, which is fast enough at ordinary volume. |

### 3.3 Control-plane / data-plane seam

The seam is **logical, not a network/language boundary** in the default deployment:

- **Control plane** (CRUD, config, RBAC, invoicing orchestration, public API, dashboard reads): `axum` +
  `sqlx`, Postgres as sole writer, OpenAPI emitted from the API types.
- **Data plane** (ingest, ledger writes, reserve/settle enforcement): the three latency/throughput-critical
  jobs, same workspace, same `Money`/`Credit` types, callable in-process in single-binary mode and over the
  internal API when split for the hosted tier.

The contract for the hosted split is the **same versioned OpenAPI** (Decision #6) plus a small internal
RPC surface generated from the same Rust types — no hand-mirrored types, no second IDL toolchain.

---

## 4. Data flow: ingest → ledger → analytics

**Two paths, deliberately separated** (async authoritative ingest vs. synchronous enforcement decision).

### Path A — async authoritative ingest (record of truth)

1. SDK emits a usage event with a **client-owned idempotency key** (uuidv7; CloudEvents source+id semantics).
2. Public API validates (typed schema at the boundary), returns **202 fast**.
3. The event is appended to the **durable ingest queue**:
   - **Default:** a Postgres **outbox** table (transactional, idempotent on the client key).
   - **Scale-out:** Redpanda, **keyed by `tenant_id` (or `tenant_id` + bucket for whale tenants)** — *not*
     by idempotency key (see §10 fix for the perf critic's hot-partition/ordering finding).
4. The **same Rust consumer** runs an effectively-once loop regardless of source (poll → validate →
   in-batch dedup → durable-dedup check → sink insert → mark processed → advance). Invalid events go to a
   **dead-letter topic + `events_dead_letter` table** from day one — never silently dropped.
5. Sink: ClickHouse `events_raw` + `AggregatingMergeTree` minute rollups via materialized views (or, in
   Postgres-only mode, a partitioned `events` table). **Bucketed and billed on `event_time` (business
   time)** so late data self-corrects within the dispute window.
6. The **priced result of each event is reflected in the ledger by `settle`** — see §5.3 for the
   correctness rule that settle, not a third transfer, is the priced posting (resolves the perf critic's
   write-amplification finding and the financial critic's dual-ledger finding).
7. A **reconciliation job** recomputes aggregates from raw events and diffs against live aggregates, and
   (critically) reconciles **ledger-posted credits vs. invoice-queried credits** (§7.3).

### Path B — synchronous reserve/settle enforcement (hot path)

On agent-call start the SDK calls `reserve(estimate, key)`; on completion `settle(reservationId, actuals,
key)`. Full mechanics, latency budget, and the four correctness fixes are in §5.

---

## 5. Real-time enforcement — the differentiator (fully specified)

This section is rewritten to address **every critical/high enforcement finding** from the perf and
financial critics. The proposal's hand-waves ("sub-ms via Redis, also TB pending", "reconciled
continuously", "overdraft == 0") are replaced with stated sequences, a latency SLO, and an explicit
overspend-tolerance contract.

### 5.0 The single load-bearing invariant

> **The authoritative ledger balance must never go negative under any fault.** ("Overdraft count == 0"
> is replaced with this, per the perf critic — it is the only invariant that survives failover/partition.)

An LLM call, once made, is an **unrecoverable real-world side effect** (money is spent at the provider).
Therefore **enforcement happens at `reserve`, before the call**, and the durable hold is what authorizes
spend. Everything below follows from this.

### 5.1 HARD vs SOFT limits, and the accepted-overspend contract

- **HARD limits** (out of credits, grant exhausted, overdraft wall): the **durable ledger hold is the
  gate**. The agent is told "go" **only after** the durable pending hold is recorded. **Fail-closed** on
  store uncertainty. **Accepted transient overspend = 0.**
- **SOFT limits** (periodic budgets like 400 credits/week, rate smoothing, spend alerts): may use leased /
  node-local counters and **fail-open** with a conservative local fallback. **Accepted overspend = one
  in-flight lease quantum** (a bounded, documented, alerted number — not "hope").

This explicit tolerance table is the perf critic's required revision: we **stop claiming zero overspend in
the distributed case** and instead state it per class.

### 5.2 The reserve sequence (crash-safe, both default and accelerated)

**Default (Postgres ledger, no Redis) — the common case:**

```
reserve(account, estimate, reservation_id):
  in one Postgres transaction:
    take a transaction-scoped advisory lock on the *leased sub-balance* for this session
      (NOT the shared org pool row — see §5.4 sharding)
    insert a PENDING ledger hold (idempotent on reservation_id)
    derive available = pool_credits - sum(open holds) - settled_debits
    if available < estimate for a HARD limit -> ROLLBACK, return DENY (fail-closed)
    COMMIT  -> the hold is durable -> return ALLOW
```

This is a single-region, single-store, two-hop decision. At ordinary self-host volume it is a few ms —
acceptable, and **fast enough that Redis is unnecessary** (the ops critic's point; Redis is removed from
the default).

**Accelerated (opt-in Redis + TigerBeetle, for high-TPS hosted tenants):**

The proposal's fatal ambiguity ("sub-ms via Redis AND a TB pending transfer") is resolved by the
financial critic's rule — **for HARD walls, Redis may only ever be *more conservative* than the durable
ledger, never less:**

```
reserve(account, estimate, reservation_id):   # HARD limit
  1. Redis Lua: tentatively check-and-decrement the leased counter (sub-ms, single round trip)
  2. durable hold: TigerBeetle pending transfer (debits_must_not_exceed_credits, DB-managed timeout)
  3. on TB success      -> tell agent "go"
     on TB failure/uncertainty -> Lua-compensate (re-increment, idempotent on reservation_id)
                                   and DENY (fail-closed)
```

Redis is an **optimistic pre-filter that shaves load off the durable gate**, never the authority. The
"sub-ms" claim is honestly scoped: **the Redis pre-check is sub-ms; the durable allow is gated on the
hold.** We therefore publish a **latency SLO table** (target / fail action), e.g.:

| Operation | p50 | p99 | p100 / on store timeout |
|---|---|---|---|
| SOFT gate decision (leased) | <0.3 ms | <1.5 ms | fail-open, local fallback |
| HARD reserve (Redis pre-check) | <0.5 ms | <2 ms | proceed to durable gate |
| HARD reserve (durable hold, accel) | <5 ms | <25 ms | **fail-closed (DENY)** |
| HARD reserve (durable hold, Postgres default) | <3 ms | <15 ms | **fail-closed (DENY)** |
| settle | <5 ms | <25 ms | retry idempotently; never drop |

(Targets are the **design contract** to benchmark against from Phase 1 — not Phase-5 marketing; §9/§13.)

**Reconciliation direction (financial critic):** Redis is **never blind-overwritten from the ledger**.
Reconciliation replays **un-acked reservation ids** so Redis-only-but-not-yet-durable holds are preserved,
not erased (erasing them would re-open budget = overspend on a HARD wall). On any Redis uncertainty
(failover, reconnection, script reload after restart) the gate **forces a durable round-trip** until
reconciled.

### 5.3 settle: estimate≠actual, write-amplification, and settle-after-void

Three correctness fixes, one per critic finding:

**(a) settle IS the priced posting (kills 1/3 of ledger write load).** There is **no separate "third
priced-result transfer."** `settle(actual)` posts the pending hold *at the actual amount* (partial-post
auto-returns the remainder). This directly removes the perf critic's "3 transfers/event" amplification
*and* removes the financial critic's "two rating paths can diverge" risk — there is one place actuals are
priced and posted.

**(b) Under-estimation is a defined ledger mechanic, not a footnote (financial critic, critical).** A
pending hold cannot post *more* than its reserved amount, and reasoning/tool-fan-out/cache-write can make
actual > estimate. v1 policy, stated and tested:

- **Reserve the true worst case** by default: `max_output + max_reasoning + max_tool_budget + cache-write`
  at the model's rate. This favors the never-overdraft promise.
- If actual still exceeds the hold (rare tail): settle posts a **second, explicit overage debit** that is
  allowed to drive the account into a **bounded, alerted `overage` sub-account** — *never* a silent
  margin-eating cap and *never* a silent negative on the main pool. **The call already happened; spent
  money must be recorded.** CI test: `settle(actual > reserved)` records the charge **exactly once**,
  never silently loses COGS, never silently negates the main pool.

**(c) settle-after-void must not lose money (financial critic, medium).** Holds have **DB-managed
timeouts** so abandoned agents auto-void. But a legitimately long agent run can exceed the timeout, the
hold auto-voids, then `settle` arrives. Rule: when `settle` finds the hold already voided/expired, it
posts a **fresh direct debit** (idempotent on `reservation_id`) into the `overage` sub-account if needed —
recorded exactly once. Additionally, **long-running calls heartbeat to extend the hold timeout**, and
**timeouts default from the model/product p99 call duration**, not a global constant.

### 5.4 Hot-account contention & over-reservation capacity (perf critic, critical + high)

Two findings, one mechanism: **per-session leasing.**

- **Hot-account writes:** instead of millions of reserves/settles against one shared org/team pool row,
  the ledger **leases a chunk of credits from the pool to a per-agent/per-session sub-balance in one
  operation**, and the hot per-call decrements happen against the *leased sub-balance* (Postgres
  advisory-lock on the sub-row, or Redis/TB sub-account when accelerated). Leases settle back periodically.
  This converts millions of hot-row writes into thousands. (Note: TigerBeetle's single-write-core design
  already tolerates hot accounts far better than Postgres, so leasing is *mandatory* for the Postgres
  default and an *optimization* under TB.)
- **Over-reservation starving budget:** reserving raw `max_tokens` worst-case per call would lock most of
  a small weekly budget in holds under concurrency (busier tenant → more false denials — a self-inflicted
  ceiling). Mitigations: (1) **statistical reservation** — reserve **p95 expected cost** per (model,
  product) learned from ClickHouse history, with a small margin, relying on the §5.3(b) overage backstop
  for the tail; (2) **leases isolate one runaway agent** from starving the tenant; (3) **short, aggressive
  hold timeouts** sized to call latency. Documented relationship:
  `outstanding-hold-ceiling = concurrency × avg-reservation` must stay well under budget for expected
  concurrency, and this is a **load-test exit criterion in Phase 1**, not a tuning afterthought.

### 5.5 The CI correctness gate (extended with fault injection)

The proposal's "N concurrent reservers, zero overdraft" passes against one healthy store and proves
nothing about the failure modes that actually overdraft. Replaced (perf + financial critics) with:

> **Under N concurrent reservers WITH injected faults** — kill the durable store's leader mid-reservation,
> restart from disk, partition Redis↔ledger, drop/duplicate settle callbacks, fire hold-timeout races —
> **the authoritative balance is never negative, no hold is leaked, and every settled call is charged
> exactly once.** Run against **both** ledger backends (Postgres default and TigerBeetle accelerator) as a
> hard gate. (proptest + testcontainers + a chaos harness; this is a Phase-1 deliverable, not Phase-5.)

### 5.6 Enforcement gateway deployment

Offered as an **optional Envoy ext-authz service AND a direct SDK call** — **never a mandatory proxy that
fails closed for all traffic** (a mandatory fail-closed proxy can take down every agent if the gate is
down). Per-limit fail-open/closed policy + circuit breaker (open if store error rate exceeds a threshold
over a short window), with documented **per-store** degradation behavior, not one global switch.

---

## 6. Pricing, rate cards, grants, budgets

### 6.1 Rate-card data model

LLM pricing is irreducibly multi-dimensional. The model is a **normalized price-component matrix**, but
the COGS catalog and customer card are **one schema with a `kind` + `margin`** (ops critic's
collapse-the-two-catalogs fix), and **all JSONB is schema-validated at the write boundary** (no opaque
"properties" bag like Lago):

- `model(id, provider, model_id, family, context_window_tokens, max_output_tokens, capabilities jsonb)`
- `rate_card(id, org_id, code, kind ∈ {provider_cost, customer}, currency, version int, effective_start,
  effective_end)` with a stable handle + resolvable `latest` pointer (replaces Metronome's alias object).
- `price_component(id, rate_card_id, model_id, meter_id, dimension ∈ {input_uncached, cache_read,
  cache_write, output, reasoning_output}, modality ∈ {text,image,audio,video,none}, context_tier ∈
  {standard, long_200k, long_1m}, ttl ∈ {none, ttl_5m, ttl_1h}, unit ∈ {token,char,image,second,call},
  charge_model ∈ {standard,graduated,volume,package,percentage,dimensional}, price_micros bigint,
  tier_bands jsonb, margin)` — the matrix.
- `action_charge(id, rate_card_id, action ∈ {web_search,code_execution,agent_session,file_search}, unit,
  price_micros, free_allotment, allotment_period)` — per-action/duration costs.
- `customer_rate_override(customer_id, rate_card_id, target, override jsonb)` — single override table; no
  tag-based indirection in v1.

**Two-stage token→credit translation:** event → COGS via the `provider_cost` card → credits via
`margin` + the credit's fixed cent value. **Store both `cost_micros` (COGS) and `credits_charged`** on
every ledger entry so margin is reconcilable per event. **Round once** at the credit/invoice-line layer,
never per token. **Context tier is computed at ingest** from total input.

**Custom aggregations** are a schema-validated AST compiled to SQL — **never stored eval'd code.**

**Pricing simulation** is first-class: re-rate real historical ClickHouse events against a *proposed*
rate card before scheduling it.

### 6.2 Grants vs. budgets — distinct ideas, one ledger (financial critic, high)

The financial critic's sharpest correctness fix: **do not track budget consumption as a separate mutable
Postgres counter** (`budget_cycle.consumed_credits`) that must be kept in cross-store sync with the ledger
on the hot path — that is the exact mutable-balance anti-pattern the founder is escaping, and a TB-debit /
Postgres-counter-update split with no 2PC will drift (false deny or budget overspend).

**Decision:** model both grants and periodic budgets **as ledger constructs derived from the one transfer
log**:

- A **grant / credit pool** is a ledger account (a `credit_block`).
- A **periodic budget** is **either** a ledger account with a hold-cap that is topped-up at the period
  anchor by a scheduled grant transfer, **or** a **derived check**: `SUM(usage transfers tagged with the
  budget's scope within [period_start, period_end]) ≤ limit`. Either way it is derived from the **same
  posted transfers**, never a parallel counter.
- A single agent call is therefore **one atomic ledger operation (or a linked transfer set)** that
  satisfies both grant balance and budget — **no cross-store hot-path commit.**
- `budget_cycles` in Postgres becomes a **read projection**, not an authority.
- Stated rule: **when grant and budget disagree, the stricter one denies.**

### 6.3 Credit blocks, burn order, and rev-rec (financial critic, low but real)

- `credit_blocks(... owner_scope, owner_id, granted_credits, remaining_credits, cost_basis_cents,
  conversion_rate, source ∈ {paid,promo,grant}, priority, expires_at)`.
- **Burn order:** FIFO by soonest expiry, then lowest cost-basis (Orb's algorithm) — **documented and
  configurable**, since "$0 promo before paid" is a deliberate, customer-friendly *finance policy* with
  rev-rec timing consequences, not an emergent `ORDER BY`.
- **Carry `source` (and a `revenue_recognizable` flag) onto every `ledger_entry` and `invoice_fee`** so
  the margin/ASC-606 views split COGS into **"COGS against recognized revenue" vs. "promo/marketing
  expense."** Burning $0 promo credits incurs real COGS with no revenue — that must book as promo expense,
  not negative product margin (which would corrupt the exact margin numbers this product sells).

### 6.4 Refunds, adjustments, chargebacks, corrections (financial critic, high)

Every money-reversing operation is an **explicit, append-only, double-entry ledger event referencing the
entry it reverses** (`reverses_entry_id`) — **never an in-place mutation or read-path-only overlay**:

- **Refunds** create a **new** credit block (or reversing transfer) referencing the original; **never
  resurrect an expired block** — refund-to-expired is a deliberate, alerted policy decision.
- **Corrections to a FINALIZED invoice** are issued as a separate **credit-note / adjustment invoice**
  that nets against the next period, with its own sealing posting. Finalized invoices stay immutable.
- The "read-path overlay" for in-flight corrections is **backed 1:1 by authoritative ledger entries** — it
  renders truth, it is not a parallel truth.
- Invariant: `SUM(all entries including reversals) == authoritative balance`, always.

---

## 7. Invoicing

### 7.1 Deterministic, query-based, fragment-invalidated

Invoices are **deterministic recomputes**: the same immutable events + the same pinned rate-card version
always produce the same invoice (Orb-style). Every event records the **rate-card version it priced at**.
A **dependency / fragment-invalidation graph** ensures only affected fragments recompute (designed up
front in Phase 4, not bolted on — full re-billing of millions of subscriptions otherwise is prohibitive;
this is deferred *in build order* but *designed* now).

### 7.2 State machine

`Draft → Grace → Finalized (immutable) → Void`. Finalization writes a **sealing ledger posting** so the
invoice is provably reconciled to ledger balances. Corrections are credit-notes (§6.4).

### 7.3 The binding invariant: enforced == billed (financial critic, #1 critical)

The proposal had **two records claiming authority over money** (ledger for holds/settle, ClickHouse for
the invoice) reconciled only by a job that proves ClickHouse is *internally* consistent — nothing proved
**credits debited == credits billed**. Resolution, made structural by the Postgres-first baseline:

> **The ledger's posted transfers are the billable record of truth.** The invoice is generated by
> **summing posted ledger transfers for the period** (joined to Postgres line metadata), **not** by
> independently re-rating ClickHouse events. ClickHouse is **analytics + dispute evidence**, not a parallel
> billing authority.

Because settle (§5.3a) is the single priced posting, "what we enforced" and "what we billed" are the
**same number by construction**. As defense-in-depth we still run a **mandatory reconciliation invariant**,
in CI and continuously in prod:
`SUM(credits in posted ledger transfers for period) == SUM(credits on the invoice) per account per period`,
to **0 micro-credits**, with an **alert + hard-block on invoice finalization** if they differ. The "which
ledger wins" question the proposal deferred is **answered: the ledger wins.**

### 7.4 Exactly-once across the dispute horizon (financial critic, high)

The chain is at-least-once with multiple dedup stores. The trap: a duplicate arriving *after* a finite
Redis dedup window is correctly rejected by the ledger (deterministic id) but could be accepted as a new
ClickHouse row and double-counted **if** the invoice were computed from ClickHouse. **§7.3 removes that
trigger** (billing reads the ledger, which has hard, permanent, DB-enforced dedup by deterministic id).
For analytics, an occasional un-merged ClickHouse duplicate is **cosmetic, not financial**. Where any
financial number *is* read from a store, the **dedup horizon ≥ the full billing + dispute window** (34+
days minimum; raw retention 13–24 months).

### 7.5 Grace / finalization boundary

Defaults to set (open question for the founder, §15): a short mutable Draft window, a grace window before
finalize, and an explicit policy for late usage arriving after FINALIZED (roll forward into the next
period via credit-note, do not mutate the sealed invoice).

---

## 8. Data model (concrete sketches)

### 8.1 Postgres (system of record — money + config)

**Hierarchy & RBAC**

```
organizations(id uuid pk, slug unique, default_currency, feature_flags text[])
billing_entities(id, org_id fk, code, default_currency, document_numbering, net_payment_term, tax…)
users(id, email)                                   -- global
memberships(id, org_id, user_id, status, revoked_at)
roles(id, org_id NULL=builtin, code, admin bool, permissions text[],
      CHECK (org_id IS NOT NULL OR builtin), CHECK (custom => permissions <> '{}'))
membership_roles(membership_id, role_id)
api_keys(id, org_id, hashed_value, permissions jsonb, scopes, expires_at)
-- every business table: org_id NOT NULL + index + RLS policy (see §10)
```

**Metering & pricing** — `products`, `meters` (+ `meter_filters`), `model`, `rate_card`,
`price_component`, `action_charge`, `customer_rate_override` (schemas in §6.1).

**The authoritative double-entry ledger** (this is the heart — Postgres, Decision #2/#7):

```
ledger_accounts(id, org_id, scope ∈ {org,team,user,product,session,promo,paid,budget,overage,fx_clearing},
                scope_id, kind, currency_or_credit_type, parent_id NULL,  -- supports per-session leases
                no_overdraft bool, created_at)
ledger_entries(id, org_id, account_id, paired_account_id,                 -- double-entry: every entry balances
               entry_type ∈ {grant,usage,reservation_hold,settle,partial_return,void,
                             refund,chargeback,expiration,amendment,fx,sealing},
               delta_credits numeric(30,5), balance_after numeric(30,5),  -- balance_after stored so audits don't replay
               cost_micros bigint, credits_charged numeric(30,5),         -- COGS + revenue per entry (margin)
               source ∈ {paid,promo,grant}, revenue_recognizable bool,    -- rev-rec split (§6.3)
               reverses_entry_id NULL,                                    -- reversal linkage (§6.4)
               reservation_id NULL, ref_event_id, ref_invoice_id,
               idempotency_key, created_at)                               -- append-only; never updated
credit_blocks(id, org_id, owner_scope, owner_id, granted_credits, remaining_credits,
              cost_basis_cents bigint, conversion_rate numeric(40,15),
              source ∈ {paid,promo,grant}, priority, expires_at)
balances(org_id, account_id, settled_credits, held_credits, depleted bool, last_sync_at)  -- read cache, derived
budgets(id, org_id, scope, scope_id, limit_credits, period ∈ {day,week,month,custom},
        enforcement ∈ {block,warn}, anchor)
budget_cycles(budget_id, period_start, period_end)        -- READ PROJECTION (consumption derived, §6.2)
grant_rules(id, org_id, target, trigger ∈ {interval,threshold}, interval, granted_credits,
            paid_credits, threshold_credits, target_balance, expiration)
```

**Holds** are `ledger_entries` of type `reservation_hold` with a `timeout`/`expires_at`; settle posts a
`settle` (+ optional `partial_return` or overage debit). In the **default** deployment this is all
Postgres; under the **TigerBeetle accelerator** the *same logical entries* are produced via TB pending /
post-pending / void transfers and projected back into `ledger_entries` (idempotent on transfer id).

**Invoicing & idempotency**

```
subscriptions(id, org_id, customer_id, plan_id, billing_time, anchor, subscription_at, ending_at)
invoices(id, org_id, billing_entity_id, customer_id, sequential_id,
         status ∈ {draft,grace,finalized,void}, currency, period_start, period_end, total_cents)
invoice_fees(invoice_id, product_id, price_component_id, units numeric, amount_cents,
             precise_amount numeric(40,15), source, revenue_recognizable)
invoice_credits(invoice_id, ledger_entry_id)
idempotency_keys(org_id, key, request_fingerprint, resource_type, resource_id,
                 response_snapshot jsonb, status, expires_at, UNIQUE(org_id,key))
```

**Money precision** (matches existing `meter-core`): credits = `numeric(30,5)` (and the in-code
`Credit(Decimal)` newtype); fiat = `bigint` cents + currency (and the `Money` newtype); conversion + per-
event precise amounts = `numeric(40,15)`. **Round once** at the credit/invoice-line layer.

### 8.2 TigerBeetle (optional ledger accelerator) — frozen integer taxonomy

When enabled, treated as a **public-API-grade frozen schema** reviewed once (ops critic — it cannot be
migrated in place):

- **Account** per credit pool/lease: `ledger` (32-bit) = credit-type/currency; `code` (16-bit) =
  account/transfer type (`grant|usage|reservation|refund|chargeback|fx|overage`); `user_data_128` =
  Postgres FK. Flags: `debits_must_not_exceed_credits`, `history`.
- **Transfer:** usage = posted debit; grant = credit; **reservation = pending transfer with timeout**;
  settle = `post_pending` (partial auto-returns remainder); cancel = void. **Deterministic 128-bit ids**
  (hash of idempotency key) ⇒ duplicate rejected by the DB.
- A **written runbook** defines the only safe evolution: **new ledger/code allocations, never reuse.**

### 8.3 ClickHouse (analytics — optional add-on)

- `events_raw` `ReplacingMergeTree(ingested_at)` keyed `(tenant_id, meter_id, event_time, event_id)`,
  `PARTITION BY toYYYYMM(event_time)`; columns include per-dimension token counts, modality breakdown,
  computed `context_tier`, `actions jsonb`, `charge_version`, `cost_micros`, `credits_charged`.
- `events_minute` `AggregatingMergeTree` with `sumState/countState/uniqState`; coarser daily/monthly MVs.
- `events_dead_letter` `MergeTree`: full original payload + error_code/message + timestamps (day one).
- Sharded by `cityHash64(tenant_id)` **only past the sharding trigger** (§9); single-node CH first.
- **Dedup upstream in the consumer**, **not** insert-dedup on MV-feeding tables (which desyncs MVs).
  Re-rating = deterministic `INSERT…SELECT` writing `-State` **partition-by-partition** (never `POPULATE`
  on live tables).
- **Workload isolation (perf critic, medium):** ClickHouse 2025–2026 supports **workload scheduling**
  (`max_concurrent_threads`, priorities) and per-profile resource limits; **dashboards read merged rollup
  tables (`events_minute/daily`), never raw, never `SELECT FINAL` on the hot read path**, and heavy
  billing/re-rating scans run in a **separate workload class** (or compute-compute-separated replica on the
  hosted tier) from interactive panels. Invoice queries (now §7.3 from the **ledger**, not raw CH) read
  finalized merged aggregates with an explicit cutoff; late events after cutoff go through the credit-note
  overlay, not a re-scan of open partitions.

---

## 9. Self-host: one build, opt-in scale-out (measured triggers)

**The default is the design center, not a degraded fork.** Same codebase, same Postgres migrations.

### Default — single binary + Postgres (≈95% of self-hosters, all single-tenant)
- `meter-server` (all roles in-process) + Postgres. **Optional ClickHouse** for usage analytics at volume.
- Ledger, holds, reserve/settle: **Postgres** (double-entry + advisory-lock leases, §5.2/§5.4). No Redis.
- Ingest: **Postgres outbox** consumed by the same Rust worker. No Redpanda.
- Docker Compose: **Postgres only** (optionally + ClickHouse) — matching the existing dev compose and the
  README promise. (The proposal's "Lite = Postgres + Redis" is corrected: **no Redis in the default**.)

### Scale-out — opt-in backends behind stable traits, each with a stated trigger
| Add | Trigger (documented number, validated by load test) |
|---|---|
| **ClickHouse** | Usage-event volume / analytics query latency exceeds what Postgres partitions serve comfortably (target threshold set in §13). |
| **Redpanda** | Sustained ingest exceeds the Postgres-outbox drain rate (target threshold in §13). |
| **Redis gate** | A tenant measurably needs sub-few-ms enforcement decisions the Postgres gate can't meet. |
| **TigerBeetle** | Ledger write-TPS exceeds a tuned Postgres primary with leasing (DECISIONS.md: "OpenAI ran 800M users on one primary" — this is *far* out). |
| **ClickHouse sharding** | Single-node CH exceeds its storage/scan budget (threshold in §13). |
| **compio / io_uring** | A *profiled* disk/ingest bottleneck — keep the I/O trait seam (cheap), ship the tokio impl only. |

Everything past the default is **deferred in build order behind a measured trigger** (perf critic's
"over-engineering for v1" finding accepted), with one exception the perf critic themselves flagged as
load-bearing: **per-session leasing for hot accounts is in v1**, because it is what makes the core
throughput claim true under contention, not a nice-to-have.

### One ledger code path, proven equivalent (resolves the Lite/Full divergence trap)
The proposal signed up for Lago's dual-pipeline divergence (two ledgers, two correctness regimes). We do
**not** ship two ledgers as co-equals. **Postgres is the one ledger everywhere.** TigerBeetle is an
**optional backend behind a narrow trait** (`post / reserve / settle / void / expire / balance`) that must
pass:
1. the **shared conformance suite** (every enforcement edge case: partial settle, hold timeout, idempotent
   retry, FIFO block expiry) run against **both** backends, and
2. a **published bill-equivalence test**: the same event stream produces **byte-identical invoices** under
   Postgres-only and under the TigerBeetle accelerator.

**If bill-equivalence cannot be guaranteed, the TigerBeetle backend does not ship.**

### Upgrade & migration (ops critic, high)
- **Postgres is the only store with a real reversible expand/contract migration story**, and **all money-
  truth schema lives there** (forward-only, SQL-first, batched/rate-limited backfills, statement timeouts —
  per the existing CLAUDE.md OpenAI Postgres discipline).
- **ClickHouse is a rebuildable derived store** — rollups can be dropped and re-derived from raw events, so
  its DDL is non-load-bearing.
- **TigerBeetle, if enabled, has a frozen taxonomy** with a written safe-evolution runbook (§8.2).
- A single **`meter migrate`** command orchestrates the ordered upgrade and **refuses to proceed on
  version skew.** If we can't give self-hosters a one-command, rollback-safe upgrade, the multi-store
  topology is not offered to them — it stays hosted-tier.

---

## 10. Multi-tenancy & ingest-log keying

**Multi-tenancy:** shared-schema with `NOT NULL org_id` on every table, **backed by Postgres RLS** as
defense-in-depth (ENABLE+FORCE; app role without BYPASSRLS; `SET LOCAL app.current_org` in a `withTenant`
transaction; transaction-mode pooling). This closes the cross-tenant-leak gap Lago left at the app layer
without making RLS the sole barrier. Single-tenant self-host is simply "one org." DB-per-tenant is an
enterprise toggle reusing identical migrations. (DECISIONS.md deferred this with shared-schema+RLS as the
expected default — confirmed.)

**Ingest-log keying (perf critic, medium):** the scale-out Redpanda topic is keyed by **`tenant_id`** (not
the idempotency key) for **locality and per-entity ordering**; dedup rides on the durable dedup store keyed
by idempotency key regardless of partition. **Whale tenants** (agent workloads are extremely skewed — a few
customers are ~90% of volume) use a **composite key `(tenant_id, hash(idempotency_key) % N)`** to spread
one tenant across N partitions while preserving locality. This hot-tenant mitigation is documented, not
discovered in prod.

---

## 11. FX / multi-currency (financial critic, medium)

**v1 is single-currency-per-org**, stated as an explicit constraint; cross-currency grants are **rejected
at the write boundary with a typed error** rather than left implicitly broken. The mechanics are designed
so multi-currency is additive later:

- The credit's fixed cent value is **locked at credit-block creation** (`cost_basis_cents` already exists)
  and that locked rate is used at invoicing — a prepaid credit's fiat value never floats.
- When cross-currency is enabled, an FX conversion is an **explicit linked transfer pair across the two
  currency ledgers via an `fx_clearing` account**, and an **`fx` entry_type / `fx_gain_loss` account** is
  added to both the Postgres taxonomy and (if enabled) the TigerBeetle code namespace, so **every FX delta
  is booked, never absorbed** (unbooked FX deltas are a textbook ledger imbalance auditors catch).

---

## 12. SDK design

Single Rust FFI core (one place the OTel-span auto-emit + durability logic lives) → TS (NAPI-RS) and
Python (maturin/pyo3), **TS + Python first**:

- **Channel A — billing (authoritative):** custom JSON-over-HTTP/2 batch ingest, **client-owned
  idempotency keys**, **disk-backed durable WAL queue**, exponential backoff + full jitter. **Never bill
  off OTLP** (OTLP is lossy/aggregated/sampled).
- **Channel B — observability (optional):** OTLP. Adopt the **OTel GenAI `gen_ai.*` attribute vocabulary
  verbatim** (input/output/cache_read/cache_creation/reasoning tokens) — maps exactly to meter's
  dimensions and gives instant Datadog/Langfuse interop.
- **Explicit `reserve()` / `settle()` hot-path API** with client-side timeouts and circuit breakers.
- Auto-wrap OpenAI / Anthropic / Bedrock / Vertex + Vercel AI SDK / LangChain.
- **Types are codegen'd from the backend OpenAPI** (Decision #6); runtime (durability, batching,
  instrumentation) is hand-written — codegen types only, not the whole SDK.

---

## 13. Performance contract (defined NOW, benchmarked from Phase 1)

The perf critic's strongest cross-cutting point: "millions of events/sec" and "sub-ms" were never made
falsifiable, and load testing was pushed to Phase 5 (too late — wrong assumptions bake into early phases).
**A one-page SLO/throughput contract is a Phase-0 deliverable** with workload-skew assumptions (Zipfian
tenants; `max_tokens` ≫ actual distribution; agent-call concurrency). Each phase's **exit criteria include
hitting its slice of the SLO under fault injection**, so a wrong assumption surfaces in the phase that made
it. Contract dimensions to fill with target/stretch numbers:

- Ingest sustained TPS and burst (Postgres-outbox default **and** Redpanda scale-out).
- Gate decision p99 (SOFT); HARD reserve p99 (Postgres default and accelerated); settle p99.
- Ledger transfers/sec under **realistic hot-account skew** (Zipfian on org pools), with and without leasing.
- ClickHouse month-end invoice/re-rate scan latency for a large tenant (and the **trigger numbers** in §9).
- Invoice generation time for a tenant with N events.

The **load + chaos harness moves to Phase 1 (ledger) and Phase 2 (ingest)** — the ledger hot path and hot-
account behavior are benchmarked the moment the ledger exists, because they gate the whole architecture.

---

## 14. Licensing & contribution

- **AGPL-3.0** core (Decision #5, already in `LICENSE`) + a thin commercial enterprise tier (SSO, advanced
  RBAC, multi-region, hosted catalog sync).
- **Contribution: DCO sign-off, not a mandatory CLA, for v1** (ops critic, low). A mandatory CLA measurably
  depresses drive-by contributions and, combined with the language barrier, squeezes the funnel from both
  sides; DCO preserves provenance with far less friction. If the company later needs relicensing rights for
  the enterprise build, scope a **narrow, automated CLA (CLA-assistant bot)** then. The **AGPL-core vs
  enterprise line is published before the first outside contribution** so contributors know what they build.
- The single biggest lever on contributor accessibility is **not** the license — it is **not forcing
  contributors to learn both Rust and Effect**, which the all-Rust backend (§3.1) already secures.

---

## 15. How each major adversarial critique was addressed

**Procedural (the meta-finding).** The proposal reversed locked Decisions #1, #2, #4 and the TigerBeetle /
Redpanda / Redis deferrals. **Reverted to the locked baseline:** one Rust backend / one binary / one money
type; Postgres as the system of record for money; ClickHouse/TigerBeetle/Redpanda/Redis as opt-in backends.
The global Effect preference is honored where it applies — TypeScript SDK + dashboard — per the repo CLAUDE.md.

**Performance & scalability.**
- *Hot-path latency budget (critical):* one mechanism stated, with a published SLO table; the durable hold
  is the gate for HARD limits; Redis (when enabled) is an explicitly-scoped sub-ms pre-filter that can only
  be *more* conservative (§5.1–5.2).
- *Redis↔durable reconciliation race (critical):* no blind overwrite; replay un-acked reservation ids;
  fail-closed on uncertainty; accepted-overspend stated per class; CI invariant rewritten to "balance never
  negative under fault injection" (§5.2, §5.5).
- *Ledger write-amplification (critical):* settle IS the priced posting (−1/3 load); per-session leasing
  converts hot-account writes to thousands; TB's single-write-core design tolerates hot accounts (refutes
  the contention worry); benchmark with Zipfian skew (§5.3a, §5.4, §13).
- *Lite is the real default (high):* made the default the design center and *correct*; one ledger code path;
  conformance + bill-equivalence gate (§9).
- *Over-reservation (high):* statistical p95 reservation + leases + short timeouts; outstanding-hold ceiling
  is a Phase-1 exit criterion (§5.4).
- *ClickHouse mixed workloads (medium):* workload scheduling + dashboards read rollups not raw; invoice now
  reads the ledger, not raw CH (§8.3, §7.3).
- *Ingest-log keying (medium):* key by tenant_id + whale-tenant composite key (§10).
- *Tail-latency / failure surface (medium):* default hot path is two hops (gate + Postgres); CDC chain
  removed from day one — the ledger writes its own projection idempotently (§3, §5).
- *Over-engineering (medium):* compio/sharding/lease-tier/re-billing-graph deferred behind measured
  triggers; leasing kept (load-bearing) (§9).
- *No targets (high):* performance contract is Phase-0; harness moves to Phases 1–2 (§13).

**Operations / self-host / simplicity.**
- *Five stateful systems (critical):* default is **two** (Postgres [+ ClickHouse]); three are opt-in (§9).
- *Two backend languages (critical):* reverted to one Rust backend; Protobuf/Buf spine deleted (§3.1).
- *TigerBeetle authoritative (critical):* demoted to an optional accelerator behind a trait; Postgres stays
  authoritative (§3.2, §8.2, §9).
- *Rate-card config sprawl (medium):* one card type + margin; provider templates; defer pricing-unit
  indirection; measure time-to-first-correct-invoice (§2, §6.1).
- *Lite/Full divergence (high):* one ledger path; conformance + published bill-equivalence gate (§9).
- *Redpanda / Redis mandated (medium):* both removed from the default; opt-in with triggers (§9).
- *Heterogeneous upgrades (high):* money-truth schema only in Postgres; CH rebuildable; TB taxonomy frozen
  with a runbook; one `meter migrate` that refuses on version skew (§9).
- *CLA friction (low):* DCO for v1; narrow automated CLA only if needed later (§14).

**Financial-correctness / ledger.**
- *Two authoritative ledgers (critical):* invoice sums the ledger (single source of truth by construction);
  mandatory enforced==billed reconciliation with hard-block on mismatch; "ledger wins" stated (§7.3).
- *Reserve < actual (critical):* worst-case default reservation; explicit bounded/alerted overage debit;
  "spent money is always recorded"; CI test (§5.3b).
- *Refunds/adjustments/chargebacks (high):* append-only reversing entries with `reverses_entry_id`; finalized
  invoices immutable; corrections as credit-notes; refund never resurrects an expired block (§6.4).
- *Redis/ledger ordering & reconciliation (high):* exact crash-safe sequence; durable hold gates HARD; no
  blind overwrite; fault-injection CI (§5.2, §5.5).
- *Exactly-once across stores (high):* billing reads the ledger (hard DB dedup); CH duplicates cosmetic;
  dedup horizon ≥ dispute window for any financial read (§7.4).
- *Budget vs grant reconciliation (high):* budgets derived from the one transfer log, never a parallel
  mutable counter; stricter denies (§6.2).
- *Hold-timeout vs settle race (medium):* settle-after-void posts a fresh idempotent debit; hold heartbeat;
  timeouts from model p99 (§5.3c).
- *FX (medium):* single-currency-per-org v1 with typed rejection; locked credit cent value; FX clearing +
  gain/loss account designed for later (§11).
- *Promo/FIFO rev-rec (low):* `source`/`revenue_recognizable` on every entry; configurable documented burn
  order; margin view splits promo expense from product margin (§6.3).

---

## 16. Final default-topology diagram (logical)

```
                         ┌─────────────────────────────────────────────┐
   SDK (TS/Python) ─202─▶│  meter-server (one Rust binary)              │
   reserve()/settle() ──▶│   ├ public API (axum + OpenAPI)              │
                         │   ├ enforcement (reserve/settle, leases)     │
                         │   ├ ledger (double-entry)                    │──▶  PostgreSQL
                         │   ├ ingest worker (effectively-once)         │     (money + config
                         │   ├ scheduler / webhooks / invoice renderer  │      + ledger + outbox)
                         │   └ catalog scraper                          │
                         └─────────────────────────────────────────────┘
                                          │ (optional)
                                          ▼
                                     ClickHouse  (usage firehose + rollups + analytics)

  OPT-IN SCALE-OUT (behind stable traits, measured triggers):
    Redpanda  → ingest log (replaces Postgres outbox above a TPS trigger)
    Redis     → sub-ms enforcement pre-filter (HARD: only ever more conservative)
    TigerBeetle → balances/holds accelerator (must pass conformance + bill-equivalence)
    ClickHouse sharding / compio io_uring → past profiled single-node limits
  Dashboard (Next.js/React) reads rollups (never raw on the hot path); types codegen'd from OpenAPI.
```

This is the locked v1 baseline: the **simplest correct deployment that meets the performance bar**, with
every system the performance critic wants available as an opt-in, and every financial-correctness fix made
structural by keeping **one ledger as the single source of truth**.
