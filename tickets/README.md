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
- [~] Migration tooling: embedded `sqlx::migrate!` done; `meter migrate` CLI (refuses on version skew) pending
- [~] Ledger schema: ledger_accounts, append-only ledger_entries, ledger_holds done; credit_blocks + balances cache + session-lease accounts + cost_micros/credits_charged pending
- [ ] Event schema: events (custom-field JSONB schema-validated, run_id, status, supersedes_event_id), events_dead_letter, idempotency_keys
- [ ] `org_id NOT NULL` everywhere + RLS (ENABLE/FORCE, app role w/o BYPASSRLS, withTenant)
- [~] Forward-only expand/contract done; statement timeouts + batched/rate-limited backfills pending
- [x] Integration tests vs real Postgres (testcontainers) — ledger conformance green

## EPIC 03 — Ledger: Postgres backend
- [x] `meter-store-pg` implements `LedgerBackend` over Postgres (FOR UPDATE serialization, derived balances)
- [x] Run the shared conformance suite against Postgres — identical results to the in-memory oracle
- [x] Concurrency no-overdraft test (50 racing reserves vs capacity) green against real Postgres
- [ ] Per-session credit leasing (hot-account mitigation)
- [ ] Chaos/fault-injection harness: leader kill, restart, dup/drop settle, hold-timeout race
- [ ] Hold timeouts (auto-void), settle-after-void overage path, heartbeat extension

## EPIC 04 — Pricing & rate cards
- [x] `meter-pricing`: rate_card (kind: provider_cost|customer, margin), price_component matrix (dimension/modality/context_tier/unit/charge_model)
- [x] Two-stage token→credit translation (cost → margin → credits via credit cash value); round once at the credit layer
- [ ] action_charge (per-action/duration), graduated/volume/package charge models, ttl tiers
- [~] Versioned rate cards (version field present); resolvable `latest` + per-event priced-version recording pending
- [ ] Pricing simulation (re-rate historical events against a proposed card)
- [ ] Schema-validated pricing config (AST for custom aggregations, never eval)

## EPIC 05 — Enforcement
- [x] `meter-enforcement`: reserve→settle orchestration over LedgerBackend, priced via rate cards; HARD/SOFT via LimitClass; void for failed runs
- [~] Worst-case reservation default done; statistical p95 + bounded/alerted overage sub-account pending
- [ ] Latency SLO instrumentation (per SLO.md); circuit breaker; fail-closed (HARD) / fail-open (SOFT) wiring at the engine API

## EPIC 06 — Ingest & event model
- [x] `meter-event`: EventStore trait + in-memory reference + conformance — custom-field events, idempotency on (org,key), amend-as-new-version, void_run, latest-non-voided reads
- [x] Postgres EventStore backend (events table, JSONB props, run_id, status, supersedes) + conformance against real PG
- [~] Event API on the engine (record/get/list/amend/void_run) done; 202-fast batch + per-meter schema validation pending
- [ ] Compose void_run with the ledger (reverse a run's holds/settles); event amend → delta posting
- [ ] `meter-ingest`: `IngestSource` trait; Postgres-outbox default source; effectively-once consumer; dead-letter
- [ ] Reconciliation job (aggregates vs raw; ledger vs invoice)

## EPIC 07 — Analytics store (ClickHouse, optional add-on)
- [ ] `meter-store-ch`: events_raw (ReplacingMergeTree), minute/day AggregatingMergeTree MVs, events_dead_letter
- [ ] Idempotent ingest (dedup upstream), deterministic re-rating (INSERT…SELECT partition-by-partition)
- [ ] Query API for dashboards (read rollups, never raw on hot path), workload isolation

## EPIC 08 — Engine binary & CLI
- [x] `meter-api` HTTP surface: accounts (open/balance/grant/entries), reservations (reserve/settle/void), health; typed error→HTTP mapping
- [x] `meter-engine` binary: serves HTTP over Postgres, runs migrations on boot, env config (METER_DATABASE_URL/METER_LISTEN_ADDR), tracing
- [x] e2e HTTP test (open→grant→balance→reserve→settle→deny) green against real Postgres
- [ ] gRPC surface (proto) for control-plane RPC; role-selectable services
- [ ] OpenAPI emission + typed client codegen
- [~] `meter-cli` (`meterctl`): `migrate` command done (idempotent, env-configurable via METER_DATABASE_URL); seed + more admin ops pending

## EPIC 09 — Control plane (TypeScript: Effect + Drizzle)
- [~] `apps/control-plane`: Effect HTTP API (`HttpRouter`, one module per resource) over Drizzle — health, organizations, products, notifications (raise/pull/read/ack), alert rules (create/list/enable) with `Schema`-validated bodies/query/path params and typed-error→JSON mapping (400/404/500); `Database` service (Postgres in prod, PGlite in tests); shared repository error channel; e2e-tested via in-process test server + `HttpClient` (11 tests). RLS/`withTenant`, RBAC/API keys, gRPC-to-engine pending
- [ ] Resources: orgs/teams/users/roles (RBAC), API keys, products/agents, rate-card config, budgets/grants config, webhooks, invoices
- [ ] gRPC client to engine for all money/usage ops; never computes money
- [ ] Auth (sessions + API keys), authorization (RBAC), OpenAPI emission; agent-doctor in CI

## EPIC 10 — Invoicing
- [~] Deterministic, query-based invoice summed from the ledger (enforced==billed): `GET /v1/accounts/:id/invoice?start&end` done & e2e-tested. Hard-block-on-mismatch reconciliation pending
- [ ] State machine Draft→Grace→Finalized(immutable)→Void; sealing posting; credit-notes for corrections
- [ ] Billing periods independent of budget cycles; line items, drawdown, rev-rec view

## EPIC 11 — SDKs & integrations
Strategy: base typed clients **generated by Stainless** from the engine OpenAPI; thin hand-written
**adapters** auto-instrument the major AI clients. See `docs/SDKS.md`.
- [x] Interim TS base client + run governance (`withRun`: reserve→settle, auto-void on failure) — tested
- [x] TS provider usage adapters (Anthropic/Claude + Agent SDK, OpenAI, Vercel AI SDK, Gemini/Vertex, Bedrock): normalize usage, `recordModelUsage`, `meteredCall`, `meterModelUsage` (price+charge via `/v1/usage`) — 13 tests
- [x] Interim Python SDK + adapters (Anthropic/Claude, OpenAI, Gemini/Vertex, Bedrock) + `meter_model_usage` + `with_run` governance — stdlib-only, 11 tests
- [ ] OpenAPI emission from the engine (code-generated via utoipa) — prerequisite, see EPIC 01
- [ ] Stainless-generated base clients (TS/Python/Go) replacing the interim TS client; CI regen on spec change
- [x] LangChain/LangGraph adapter (`langchainUsage` / `langchain_usage`) in both SDKs, tested
- [ ] First-class per-client auto-patch wrappers (monkey-patch a provider client)
- [ ] e2e SDK tests against a running engine

## EPIC 12 — Hosted model rate-card catalog
- [~] `meter-ratecards`: curated, dated snapshot catalog (CATALOG_AS_OF) that builds provider-cost rate cards from a model id; Anthropic flagship models seeded; tested
- [ ] Scraper + scheduled auto-update, versioned immutable snapshots, diff-and-alert, more providers (OpenAI/Google/DeepSeek/Qwen), manual override; serve via the catalog/control-plane API

## EPIC 13 — Notifications, alerts & webhooks
- [~] Budget/alert status (read side): `GET /v1/accounts/:id/budget?...&limit` → usage vs limit + threshold status (ok/warning≥80%/exceeded≥100%); e2e-tested. Persisted alert rules + delivery below
- [~] Alert rules persisted in the control plane: create/list/enable-disable over `scope` (org/team/user/product) × `metric` (budget/credit/spend) × `action` (notify/webhook/enforce), `Schema`-validated; e2e-tested. Threshold-evaluation loop (cross usage → raise) pending
- [~] Alert actions: webhook delivery shipped — signed (HMAC-SHA256 `X-Meter-Signature`), retried with backoff, event-type filtered, raised automatically when a notification is created, with an append-only delivery log; e2e-tested against a live sink. Email/in-app notify + enforce(block) pending
- [x] Notifications as first-class records: raise + pull/list (filter by status) + mark-read + ack via the control-plane API; e2e-tested. Snooze/top-up reactions pending
- [ ] Subscriptions ("notify me when …") per user + delivery preferences
- [x] Dead-letter for failed webhooks (failed deliveries recorded with attempts/error); IaC config + idempotent/async delivery queue pending

## EPIC 17 — Audit log
- [x] Engine audit log: middleware records every mutating request (actor via `x-meter-actor`, method, path, status, time); `GET /v1/audit` lists newest-first; e2e-tested
- [ ] Before/after diffs + request-id correlation; control-plane actions; filter by actor/resource/time + export
- [ ] Tamper-evident (hash chain)

## EPIC 14 — Dashboard (Next.js + design system)
- [x] Scaffolded `apps/dashboard` via shadcn preset `b1z2hUjZ5c` (Next.js 16 App Router/RSC, base-ui, phosphor, Tailwind v4, Dropbox aesthetic, dark mode); bun-managed; CI job runs typecheck + lint + build
- [~] App shell (sidebar nav + org switcher) and screens wired to the control plane: Overview (stat cards + recent notifications), Organizations, Notifications (pull/filter/read/ack via server actions + toasts), Alert rules (enable/disable), Webhooks (endpoints + delivery dead-letter log, enable/disable). Usage/credit-burn charts, accounts hierarchy, rate cards, budgets/grants, invoices, events explorer (+amend/void), settings/RBAC pending
- [~] Typed control-plane fetch client (degrades gracefully when the control plane is down). Generated client from OpenAPI, transitions.dev animations, react-doctor + Lighthouse budgets in CI pending

## EPIC 15 — Deployment & self-host
- [~] Engine Docker image (multi-stage, rustls, slim runtime) + production docker-compose (Postgres + engine) — built & smoke-tested. control-plane/dashboard images + ClickHouse service pending
- [ ] Helm chart (scale-out); private-VPC notes; config/secrets; `meter migrate` orchestration
- [ ] Single-tenant & multi-tenant modes; opt-in scale-out backends behind traits

## EPIC 16 — Cross-cutting: security, observability, e2e, benchmarks
- [ ] RBAC + tenant isolation tests; secrets handling; dependency/audit in CI
- [ ] Tracing/metrics/logs across engine + control plane
- [ ] **Full e2e**: SDK → engine → ledger → control plane → invoice → dashboard, against real stores (testcontainers / compose)
- [ ] Throughput + latency benchmarks (criterion + load harness) meeting SLO.md; published results
- [ ] DCO, CONTRIBUTING, SECURITY, issue/PR templates; README/docs kept current every epic
