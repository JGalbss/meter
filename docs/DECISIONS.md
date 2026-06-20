# Decision log

Running log of locked decisions. The full reasoning and system design live in `ARCHITECTURE.md`;
this is the quick-reference index of what was decided and why.

| # | Decision | Rationale | Status |
|---|---|---|---|
| 1 | **Rust** for the entire backend (control plane + data plane), not Zig | Performance + memory safety + mature ecosystem (axum/sqlx/tokio); single language = one binary, simplest self-host, no cross-language currency drift | locked |
| 2 | **PostgreSQL** is the system of record for money & config (ledger, accounts, rate cards, budgets, grants, invoices, hierarchy) | Credit transactions are far lower-volume than raw usage; Postgres single-primary + read replicas scales very far (cf. OpenAI: 800M users on one primary) with operational discipline | locked |
| 3 | **ClickHouse** owns the high-volume usage-event firehose + analytics/rollups | OLAP fit for millions of events/sec; proven by Lago & OpenMeter; idempotent ingest via transaction_id; optional for small self-host | locked |
| 4 | **TypeScript only client-side**: Next.js dashboard + TS SDK. No TS backend | Keeps the backend single-language; UI/SDK is where TS shines | locked |
| 5 | **AGPL-3.0** license | Open and self-hostable, but network use of modifications must be shared — prevents a closed competing hosted fork | locked |
| 6 | Backend behind a versioned **OpenAPI contract**; SDK/UI types are codegen'd from it | Single source of truth; makes the all-Rust control-plane decision reversible | locked |
| 7 | **Double-entry ledger**; balances derived, never mutated; all ingest idempotent | Auditable correctness; no lost/double/overspent credits | locked |
| 8 | Real-time enforcement via **reserve → settle** (two-phase) | Stop out-of-budget agents on the hot path with no overdraft; settle with actuals | locked |
| 9 | Quality gates: clippy/rustfmt (Rust), **react-doctor** (millionco) for React, **agent-doctor** (JGalbss) for any Effect-TS | Enforce clean code automatically in CI | locked |
| 10 | Repo: **public, `JGalbss/meter`**, no AI footprint | Founder directive | locked |

## Confirmed by the ADR (`ARCHITECTURE.md`, 2026-06-19)

The research + adversarial-design pass confirmed and detailed the deferred items. All money-truth lives in
**one Postgres double-entry ledger** (the single source of truth); invoices are computed by **summing the
ledger**, so *enforced == billed by construction*. Optional scale-out backends sit behind stable traits,
each gated by a measured trigger:

| Item | Decision |
|---|---|
| Ingest log | **Default = Postgres outbox** + the same Rust effectively-once worker. Redpanda is opt-in above a TPS trigger, keyed by `tenant_id` (+ composite bucket for whale tenants). |
| Hot-path enforcement | **Default = Postgres** advisory-lock on a **per-session credit lease** (not the shared pool row). Redis is an opt-in sub-ms pre-filter that may only ever be *more* conservative than the ledger. |
| Hot-account contention | **Per-session credit leasing is in v1** (the one scale optimization kept early — it makes the throughput claim true under contention). |
| Multi-tenancy | Shared-schema, `NOT NULL org_id` everywhere, backed by Postgres **RLS** (ENABLE+FORCE). DB-per-tenant is an enterprise toggle reusing identical migrations. |
| TigerBeetle | Optional **balances/holds accelerator** behind a `LedgerBackend` trait; ships only if it passes the conformance suite **and** a byte-identical bill-equivalence test. |
| ClickHouse | First optional add-on (usage firehose + rollups + analytics + dispute evidence). **Analytics, not a billing authority.** |

## Adopted defaults for the founder's open questions (ADR §15)

The founder delegated these ("figure out the rest"). Recommended defaults adopted; revisit anytime.

1. **Effect-TS control plane?** No — stay all-Rust (reopening requires an explicit RFC against Decision #1).
2. **Reservation sizing:** worst-case for HARD pools; statistical p95 opt-in per (model, product); the bounded/alerted overage sub-account is always on.
3. **Lease quantum:** small, and shrinks toward zero as the balance nears the limit so the final credits are always enforced centrally.
4. **Invoice boundary:** short mutable Draft + grace window, then immutable finalize; late usage rolls forward via credit-note (never mutate a sealed invoice).
5. **Outcome billing:** outcomes are ordinary metered events in v1; verification/dispute flows later.
6. **Scale-out triggers:** derived from the Phase-1/2 load+chaos harness, then published — not hardcoded.
7. **Hosted catalog:** best-effort + versioned immutable snapshots + diff-and-alert + manual override; no accuracy SLA in v1.
8. **AGPL vs enterprise line:** all core metering/ledger/enforcement/invoicing is AGPL; only org-management & ops-scale features are enterprise.
9. **CLA vs DCO:** DCO sign-off for v1; a narrow automated CLA only if a closed enterprise build later needs it.

## Amendments (full records in `docs/adr/`)

- **[ADR 0001](adr/0001-engine-controlplane-split.md) — Engine / control-plane split (amends Decisions #1, #4).**
  The backend is now **two** services: a Rust **engine** (data plane, the sole owner of money-truth) and
  a TypeScript **control plane** (Effect + Drizzle on Postgres) that the frontend hits, with
  **protobuf/gRPC** between them. The control plane computes no money — it calls the engine for every
  money/usage op — so there is still exactly one ledger and no drift. TypeScript = control plane +
  dashboard + SDK; the SDK hot path talks to the engine directly.
- **[ADR 0002](adr/0002-editable-events-and-run-voiding.md) — Editable events & run voiding (extends §4/§6.4).**
  Events carry arbitrary, schema-validated custom fields. `amend_event` and `void_run` are first-class,
  append-only corrections (never in-place mutation) — Metronome-beating UX with a perfect audit trail.
  `run_id` is a core dimension across ingest, ledger leases, analytics, and invoicing.
- **[ADR 0003](adr/0003-events-in-clickhouse.md) — Events live in ClickHouse, not Postgres (sharpens Decisions #2/#3).**
  The high-volume usage-event firehose and its analytics rollups are a ClickHouse system of record;
  Postgres keeps money + config. Ingest is idempotent and analytics derive from the events SoR, so there
  is no second source to reconcile.
- **[ADR 0004](adr/0004-audit-log-in-clickhouse.md) — Audit log in ClickHouse, not the money database (extends ADR 0003).**
  The append-only audit firehose is kept off the money Postgres and lives on ClickHouse alongside events.
- **[ADR 0005](adr/0005-provider-scale-throughput.md) — Scaling to provider-grade throughput.**
  Millions of metering ops/sec without trading away the sacred ledger, attacked behind the
  `LedgerBackend` seam: per-session leasing in v1, an optional TigerBeetle backend, stateless engine
  replicas, and a ClickHouse cluster for the firehose.
- **[ADR 0006](adr/0006-wire-protocol-versioning.md) — Wire-protocol versioning policy (extends Decision #6, ADR 0001).**
  Both contracts evolve additive-only within a major (`meter.v1` proto, `/v1` OpenAPI); a breaking change
  is a new major served in parallel through a published sunset window. Enforced by `buf breaking` and the
  OpenAPI freshness + client-drift gates so an accidental break cannot merge.
- **Code organization (standing principle).** Atomic, one-concept-per-file modules; no cramming. The
  codebase will be large — keep it navigable. Enforced in review; detailed in `CLAUDE.md`.
