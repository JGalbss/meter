# meter — tickets

The living checklist of everything to build. Source of truth for "what's left." Keep it current: check
items off as they land, add tickets as scope is discovered. Detailed per-epic tickets live alongside
this file (`NN-epic.md`) and are created when an epic starts.

Conventions: `[ ]` todo · `[~]` in progress · `[x]` done. Every shipped item is green
(`fmt`/`clippy`/lint/tests) and committed atomically. No shortcuts — see `CLAUDE.md`.

---

## EPIC 00 — Foundations  ✅
- [x] Repo `JGalbss/meter` (public, AGPL-3.0, no AI footprint)
- [x] `docs/`: VISION, ARCHITECTURE (ADR), SLO, DECISIONS, adr/0001, adr/0002
- [x] Rust workspace + toolchain pin; CI (fmt, clippy -D, test)
- [x] `meter-core` — Money/Credit (exact decimal), typed UUIDv7 ids
- [x] `meter-ledger` — LedgerBackend trait + in-memory reference ledger; conservation + no-overdraft proptests
- [x] TS workspace (pnpm, strict tsconfig, Biome); design-system skill; transitions.dev installed

## EPIC 01 — Contracts (proto + OpenAPI)
- [ ] `proto/` Buf module; lint + breaking-change in CI
- [ ] Engine gRPC service defs: Ledger (grant/reserve/settle/void/balance), Ingest (event/amend/void_run), Query, Config-sync (rate cards/grants/budgets)
- [ ] Codegen: `prost`/`tonic` (Rust `meter-proto`) + `ts-proto`/connect (control plane)
- [ ] Control-plane OpenAPI emission + typed client codegen for dashboard
- [ ] Wire-protocol versioning policy

## EPIC 02 — Engine schemas & migrations (Postgres, sqlx)
- [ ] Migration tooling (`sqlx migrate`) + `meter migrate` CLI that refuses on version skew
- [ ] Ledger schema: ledger_accounts (+ session lease accounts), append-only ledger_entries (balance_after, reverses_entry_id, reservation_id, source, revenue_recognizable, cost_micros, credits_charged), credit_blocks, balances cache
- [ ] Event schema: events (custom-field JSONB schema-validated, run_id, status, supersedes_event_id), events_dead_letter, idempotency_keys
- [ ] `org_id NOT NULL` everywhere + RLS (ENABLE/FORCE, app role w/o BYPASSRLS, withTenant)
- [ ] Forward-only expand/contract; statement timeouts; batched/rate-limited backfills
- [ ] Integration tests vs real Postgres (testcontainers)

## EPIC 03 — Ledger: Postgres backend
- [ ] `meter-store-pg` implements `LedgerBackend` over Postgres
- [ ] Per-session credit leasing (hot-account mitigation)
- [ ] Run the shared conformance suite (the in-memory oracle's tests) against Postgres — identical results
- [ ] Chaos/fault-injection harness: leader kill, restart, dup/drop settle, hold-timeout race → balance never negative, no leaked holds, settle exactly once
- [ ] Hold timeouts (auto-void), settle-after-void overage path, heartbeat extension

## EPIC 04 — Pricing & rate cards
- [ ] `meter-pricing`: rate_card (kind: provider_cost|customer, margin), price_component matrix (dimensions/modality/context_tier/ttl/charge_model), action_charge
- [ ] Two-stage token→credit translation (COGS → credits via margin + credit cent value); store cost_micros + credits_charged; round once
- [ ] Versioned rate cards + resolvable `latest`; every event records its priced version
- [ ] Pricing simulation (re-rate historical events against a proposed card)
- [ ] Schema-validated pricing config (AST for custom aggregations, never eval)

## EPIC 05 — Enforcement
- [ ] `meter-enforcement`: HARD/SOFT classes; reserve→settle orchestration over LedgerBackend
- [ ] Worst-case reservation default + optional statistical p95; bounded/alerted overage sub-account
- [ ] Latency SLO instrumentation (per SLO.md); circuit breaker; fail-closed (HARD) / fail-open (SOFT)

## EPIC 06 — Ingest & event model
- [ ] `meter-ingest`: `IngestSource` trait; Postgres-outbox default source; effectively-once consumer (poll→validate→dedup→sink→ack); dead-letter
- [ ] Event API: custom fields (schema-validated per meter), client idempotency key, 202-fast, batch with per-event accept/reject
- [ ] `amend_event` (version + delta posting), `void_run` (reverse a run's holds/settles), latest-non-voided reads
- [ ] Reconciliation job (aggregates vs raw; ledger vs invoice)

## EPIC 07 — Analytics store (ClickHouse, optional add-on)
- [ ] `meter-store-ch`: events_raw (ReplacingMergeTree), minute/day AggregatingMergeTree MVs, events_dead_letter
- [ ] Idempotent ingest (dedup upstream), deterministic re-rating (INSERT…SELECT partition-by-partition)
- [ ] Query API for dashboards (read rollups, never raw on hot path), workload isolation

## EPIC 08 — Engine binary & CLI
- [ ] `meter-api` (gRPC/HTTP surface), `meter-engine` binary (role-selectable services), config/secrets, health/readiness, tracing
- [ ] `meter-cli`: migrate, seed, admin ops

## EPIC 09 — Control plane (TypeScript: Effect + Drizzle)
- [ ] `apps/control-plane`: Effect HttpApi + Effect Schema; Drizzle config schema + migrations; RLS/withTenant
- [ ] Resources: orgs/teams/users/roles (RBAC), API keys, products/agents, rate-card config, budgets/grants config, webhooks, invoices
- [ ] gRPC client to engine for all money/usage ops; never computes money
- [ ] Auth (sessions + API keys), authorization (RBAC), OpenAPI emission; agent-doctor in CI

## EPIC 10 — Invoicing
- [ ] Deterministic, query-based invoices summed from the ledger (enforced==billed, hard-block on mismatch)
- [ ] State machine Draft→Grace→Finalized(immutable)→Void; sealing posting; credit-notes for corrections
- [ ] Billing periods independent of budget cycles; line items, drawdown, rev-rec view

## EPIC 11 — SDKs (TS + Python)
- [ ] Single Rust FFI core (durable disk WAL queue, idempotency, jittered retry); NAPI-RS (TS), pyo3/maturin (Python)
- [ ] Clean API: `meter.event({...custom})`, `run.reserve/settle/void` (auto-void on error), `amendEvent`, `voidRun`
- [ ] Auto-instrumentation (OpenAI/Anthropic/Bedrock/Vertex + Vercel AI SDK/LangChain); OTel gen_ai.* optional channel
- [ ] Types codegen'd from OpenAPI/proto; e2e tests against a running engine

## EPIC 12 — Hosted model rate-card catalog
- [ ] `meter-ratecards`: scrape provider prices (Anthropic/OpenAI/Google/DeepSeek/Qwen/…), versioned immutable snapshots, diff-and-alert, manual override; self-serve "use our model rate cards"

## EPIC 13 — Notifications, alerts & webhooks
- [ ] Alert rules: thresholds on budgets/credits/spend (e.g. 80% of cap, balance < X, burn-rate spike), per scope (org/team/user/product) and event type (budget, credit, invoice, run-failure)
- [ ] Alert actions: notify (email + in-app), webhook (signed, retried, configurable URL + scopes), and enforce (hard cap = block) vs warn-only
- [ ] Notifications as first-class records: pull/list via API, mark read / ack, react (acknowledge, snooze, top-up)
- [ ] Subscriptions ("notify me when …") per user + delivery preferences
- [ ] IaC-configurable; idempotent delivery; dead-letter for failed webhooks

## EPIC 17 — Audit log
- [ ] Immutable, append-only audit log of every action a principal takes (who, what, when, before/after, request id) across control plane + engine
- [ ] Admin-facing query/filter by actor/resource/time + export
- [ ] Covers config changes, grants, voids/amends, rate-card edits, RBAC changes, auth events; tamper-evident (hash chain)

## EPIC 14 — Dashboard (Next.js + design system)
- [ ] Scaffold `apps/dashboard` via shadcn preset `b1z2hUjZ5c`; transitions.dev; Dropbox aesthetic
- [ ] Screens: usage & credit-burn (charts), accounts/hierarchy, rate cards, budgets/grants, invoices, events explorer (+ amend/void run), webhooks, settings/RBAC
- [ ] Generated control-plane client; react-doctor + Lighthouse budgets in CI

## EPIC 15 — Deployment & self-host
- [ ] Docker images (engine, control-plane, dashboard); Docker Compose (Postgres + engine + control-plane [+ ClickHouse])
- [ ] Helm chart (scale-out); private-VPC notes; config/secrets; `meter migrate` orchestration
- [ ] Single-tenant & multi-tenant modes; opt-in scale-out backends behind traits

## EPIC 16 — Cross-cutting: security, observability, e2e, benchmarks
- [ ] RBAC + tenant isolation tests; secrets handling; dependency/audit in CI
- [ ] Tracing/metrics/logs across engine + control plane
- [ ] **Full e2e**: SDK → engine → ledger → control plane → invoice → dashboard, against real stores (testcontainers / compose)
- [ ] Throughput + latency benchmarks (criterion + load harness) meeting SLO.md; published results
- [ ] DCO, CONTRIBUTING, SECURITY, issue/PR templates; README/docs kept current every epic
