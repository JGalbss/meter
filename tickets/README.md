# meter — tickets

The living checklist of everything to build. Source of truth for "what's left." Keep it current: check
items off as they land, add tickets as scope is discovered. Detailed per-epic tickets live alongside
this file (`NN-epic.md`) and are created when an epic starts.

Conventions: `[ ]` todo · `[~]` in progress · `[x]` done. Every shipped item is green
(`fmt`/`clippy`/lint/tests) and committed atomically. No shortcuts — see `CLAUDE.md`.

---

## EPIC 00 — Foundations  ✅
- [x] Repo `JGalbss/meter` (public, AGPL-3.0, no AI footprint) — canonical GNU AGPLv3 `LICENSE` file
  committed; `AGPL-3.0-only` declared in the Cargo workspace, every `package.json`, and the Python SDK.
- [x] `docs/`: VISION, ARCHITECTURE (ADR), SLO, DECISIONS, adr/0001, adr/0002
- [x] Rust workspace + toolchain pin; CI (fmt, clippy -D, test)
- [x] `meter-core` — Money/Credit (exact decimal), typed UUIDv7 ids
- [x] `meter-ledger` — LedgerBackend trait + in-memory reference ledger; conservation + no-overdraft proptests
- [x] TS workspace (pnpm, strict tsconfig, Biome); design-system skill; transitions.dev installed

## EPIC 01 — Contracts (proto + OpenAPI)
- [x] `proto/` Buf module (buf v2, STANDARD lint, FILE breaking) — **lint + breaking-change gate in CI**
  (the `proto` job runs `buf lint` and `buf breaking --against` main on every PR)
- [x] Engine gRPC service defs: Ledger (grant/reserve/settle/void/balance), Ingest (event/amend/void_run), Query (usage-by-model/field/day, event-count, invoice), Config-sync (PutRateCard/SetBudget; grants via LedgerService) — all four services defined in `proto/meter/v1`, tonic codegen in `meter-proto`, served from `meter-api`'s `grpc`; buf lint + breaking-change gate green
- [ ] Codegen: `prost`/`tonic` (Rust `meter-proto`) + `ts-proto`/connect (control plane)
- [~] Control-plane **OpenAPI emission** done: `GET /openapi.json` serves an OpenAPI 3.1 doc whose
  request bodies are generated from the routes' own Effect `Schema`s (`JSONSchema.make` — no
  hand-mirroring, no drift from validation); served + tested. **Typed responses, all resources**: every
  control-plane repo response type is now Schema-first (organizations, products, API keys, alert rules,
  notifications, webhooks, webhook-deliveries), so the doc fully describes 200/201 bodies — typed arrays
  for lists, `201 Created` for creates, `CreatedApiKey` with the one-time token. Schemas are **named
  under `components/schemas` and `$ref`'d** (codegen-ready), a checked-in `apps/control-plane/openapi.json`
  is emitted by `openapi:emit`, and a test fails if it drifts from the served doc. **Typed client
  codegen done**: the dashboard's control-plane types are generated from the spec via
  `openapi-typescript` (`gen:api` → `control-plane.gen.ts`) — the hand-mirrored types are gone, and CI
  fails if the generated client drifts from `openapi.json`. The protobuf engine⇄control-plane contract
  remains (engine has no gRPC surface yet).
- [x] Wire-protocol versioning policy — **ADR 0006**: additive-only within a major (`meter.v1` proto,
  `/v1` OpenAPI); a breaking change is a new major served in parallel through a published sunset window;
  enforced by `buf breaking` + the OpenAPI freshness/client-drift gates; deprecate-then-remove lifecycle.

## EPIC 02 — Engine schemas & migrations (Postgres, sqlx)
- [x] Migration tooling: embedded `sqlx::migrate!`, the `meterctl migrate` CLI, and **version-skew
  refusal** — `migrate` fails fast when the database has an applied migration this binary doesn't ship
  (sqlx default, never `set_ignore_missing`); proven by the `migrate_refuses_when_the_database_is_ahead`
  integration test.
- [~] Ledger schema: ledger_accounts, append-only ledger_entries, ledger_holds done; credit_blocks + balances cache + session-lease accounts + cost_micros/credits_charged pending
- [ ] Event schema: events (custom-field JSONB schema-validated, run_id, status, supersedes_event_id), events_dead_letter, idempotency_keys
- [ ] `org_id NOT NULL` everywhere + RLS (ENABLE/FORCE, app role w/o BYPASSRLS, withTenant)
- [~] Forward-only expand/contract done; statement timeouts + batched/rate-limited backfills pending
- [x] Integration tests vs real Postgres (testcontainers) — ledger conformance green

## EPIC 03 — Ledger: Postgres backend
- [x] `meter-store-pg` implements `LedgerBackend` over Postgres (FOR UPDATE serialization, derived balances)
- [x] Run the shared conformance suite against Postgres — identical results to the in-memory oracle
- [x] Concurrency no-overdraft test (50 racing reserves vs capacity) green against real Postgres
- [x] Per-session credit leasing (hot-account mitigation): `open_lease`/`close_lease` move credits via a conserving `Transfer` entry between a parent pool and a fresh `Session` child — refuses to overdraw a no-overdraft parent; `close_lease` returns `settled − held` (safe with holds open). Implemented on **both** backends and verified by the shared conformance suite (in-memory oracle + real Postgres): credits conserved, no overdraft. Exposed over the engine HTTP API (`POST /v1/leases`, `POST /v1/leases/{id}/close`) and both SDKs (`openLease`/`closeLease`, `open_lease`/`close_lease`); end-to-end conservation verified by the `lease_flow_over_http` e2e test (Postgres + ClickHouse)
- [ ] Chaos/fault-injection harness: leader kill, restart, dup/drop settle, hold-timeout race
- [x] Hold timeouts (auto-void via the expiry sweep), heartbeat extension, and the settle-after-void /
  overage paths — all conformance-tested across both backends (`expired_holds_are_swept`,
  `extend_hold_keeps_it_alive`, `settle_after_void_is_refused`, `settle_overage_charges_actual`,
  `soft_limit_allows_overage`).

## EPIC 04 — Pricing & rate cards
- [x] `meter-pricing`: rate_card (kind: provider_cost|customer, margin), price_component matrix (dimension/modality/context_tier/unit/charge_model)
- [x] Two-stage token→credit translation (cost → margin → credits via credit cash value); round once at the credit layer
- [~] Charge models: `action_charge` (per-action/duration), and graduated / volume / package all
  shipped in `ChargeModel` (tier-schedule + package-size validation) and tested. ttl (cache-duration)
  tiers still pending.
- [x] Versioned rate cards: `latest` resolution done (`latest_rate_card` → `ORDER BY version DESC LIMIT
  1`, used by `resolve_card`), and each priced event records its `rate_card_id` + `rate_card_version` +
  `priced_via` for re-derivation. Tested (`usage_prices_with_a_synced_rate_card`, store-pg `config`).
- [x] Pricing simulation (re-rate a usage stream onto a proposed card): `POST /v1/simulate` via
  `simulate_rerate`; e2e-tested (`simulate_over_http`).
- [ ] Schema-validated pricing config (AST for custom aggregations, never eval)

## EPIC 05 — Enforcement
- [x] `meter-enforcement`: reserve→settle orchestration over LedgerBackend, priced via rate cards; HARD/SOFT via LimitClass; void for failed runs
- [~] Worst-case reservation default done; statistical p95 + bounded/alerted overage sub-account pending
- [ ] Latency SLO instrumentation (per SLO.md); circuit breaker; fail-closed (HARD) / fail-open (SOFT) wiring at the engine API

## EPIC 06 — Ingest & event model
> **ADR 0003: events live in ClickHouse, not Postgres.**
> - [x] ClickHouse `EventStore` backend (`meter-store-ch`, `events` ReplacingMergeTree; status changes
>   are versioned rows, reads use `FINAL`) — **passes the shared event conformance suite against a real
>   ClickHouse container** (record/get, idempotency, amend-supersedes, void_run).
> - [x] Engine rewired to the ClickHouse `EventStore` (`/v1/usage`, `/v1/events`, ingest); ledger
>   stays on Postgres; **`PgEventStore` + the Postgres `events` table/migration removed**; ClickHouse is
>   required (`METER_CLICKHOUSE_URL`) and in the production compose. http e2e runs Postgres+ClickHouse.
- [x] `meter-event`: EventStore trait + in-memory reference + conformance — custom-field events, idempotency on (org,key), amend-as-new-version, void_run, latest-non-voided reads
- [x] Postgres EventStore backend (events table, JSONB props, run_id, status, supersedes) + conformance against real PG
- [~] Event API on the engine (record/get/list/amend/void_run) done; **202-fast batch shipped**
  (`POST /v1/events/batch` → one bulk ClickHouse insert via `record_batch`, e2e-tested); per-meter
  schema validation pending
- [~] Compose void_run with the ledger (reverse a run's holds/settles) **done** (`POST /v1/runs/:id/void`
  reverses events + ledger, e2e-tested). **event amend → delta posting**: idempotency prerequisite done
  (amend is now idempotent on an optional key, rollup-safe under retries — commit 5eeeafc); the delta
  posting itself is designed in **ADR 0009 (Proposed)** — a usage-aware re-pricing amend that posts the
  engine-computed delta — pending acceptance before implementation (it changes money-truth behaviour).
- [ ] `meter-ingest`: `IngestSource` trait; Postgres-outbox default source; effectively-once consumer; dead-letter
- [~] Reconciliation (aggregates vs raw; ledger vs invoice) — **aggregates-vs-SoR done**:
  `reconcile_rollups` diffs **every** pre-aggregated rollup against a live `events FINAL` scan — the
  `usage_rollup` (by model) and each promoted-field `field_usage_rollup` (by team/feature/…) — returning
  per-group drift tagged with its scope + dimension (empty = consistent). Surfaced as
  `GET /v1/orgs/:id/reconcile` and `meterctl reconcile --org <id>` (exits non-zero on drift, so it can
  gate a cron/alert). The **repair** side is `rebuild_rollups` / `meterctl rebuild-rollups --org <id>`:
  it clears and repopulates an org's rollups from the SoR, closing the detect→fix loop. Unit-tested
  (drift detection incl. one-sided groups) + e2e (zero drift after record/amend/void; drift injected via
  append-mode double-ingest is detected then repaired by rebuild; over HTTP and via the CLI binary).
  Still to do: ledger-vs-invoice reconciliation (needs the sealed invoice from EPIC 09).

## EPIC 07 — Analytics store (ClickHouse)
- [x] `meter-store-ch`: usage analytics (`usage_by_model`, `usage_by_day`, `event_count`) derived
  **directly from the `events` system of record** (`FINAL` + `status = 'recorded'`) — so amends count
  once at the corrected version and voided runs drop out, with no second source to keep in sync. The
  disconnected, never-written `events_raw` firehose was **removed** (no dead code). Plus
  **`events_dead_letter`** (record/list/count). Integration-tested against a real ClickHouse container
  driving the real ingest path (record/amend/void).
- [x] Idempotent ingest — `events` ReplacingMergeTree dedup on `(org_id, id)` with `FINAL`; the event
  store's `record` is idempotent on the idempotency key (proven in the integration test).
- [x] Rollup queries surfaced over the engine HTTP API: `GET /v1/orgs/:id/usage-by-model`,
  `/usage-by-day`, `/event-count`; e2e-tested (Postgres + ClickHouse) via `org_usage_analytics_over_http`.
- [x] **Perf:** rollup MV shipped — a sign-weighted `SummingMergeTree` (`usage_rollup`) maintained by a
  materialized view, so `usage_by_model`/`usage_by_day`/`event_count` avoid JSON extraction + `FINAL`
  scans and stay O(rollup groups) (single-digit ms at 1M; flat as events grow). The amend/void
  double-count problem is solved by sign-weighting (+1 recorded, −1 amended/voided; the reversal copies
  the prior row, so it cancels exactly); exactly-once ingest (`IngestMode`) stops duplicate `+1`s.
- [x] **Flexible credit burndown** — group credit consumption by any custom event field (`team`,
  `feature`, …) as well as `model`: `usage_by_field` over the `events` SoR, surfaced as
  `GET /v1/orgs/:id/usage-by-field?field=<name>` and the `QueryService.UsageByField` gRPC RPC (buf
  breaking-change check passes). e2e-tested (Postgres + ClickHouse) via
  `credit_burndown_by_custom_field_and_model` (HTTP) and `query_grpc_analytics_and_invoice` (gRPC).
- [x] **Burnable vs non-burnable usage** — `burnable` flag (default true) on `POST /v1/usage`:
  non-burnable usage is priced and recorded for cost visibility but never debits the ledger, so its
  burned `credits` are zero (burndown stays truthful) while the would-be charge is kept in
  `priced_credits`. Unit-tested (`burned_credits`, reserved-key spoof guard) + e2e
  (`non_burnable_usage_records_cost_but_never_charges`: ledger untouched, burndown truthful).
- [x] **Even faster:** promoted-field rollup shipped — a sign-weighted `field_usage_rollup`
  `SummingMergeTree` (keyed `org_id, field_name, field_value, day`), maintained by an MV that
  `ARRAY JOIN`s over a declared promoted-field set (`schema::PROMOTED_FIELDS`), so `usage_by_field` over
  a hot field reads the rollup (O(rollup groups)) instead of scanning `events FINAL` + parsing JSON per
  row. Burndown by any non-promoted field still uses the scan path. Proven byte-for-byte equivalent to
  the scan through amends (value moves) and voids by
  `promoted_field_rollup_equals_the_scan_path_through_amends_and_voids`.
  Correctness gate: the integration test asserts the rollup reflects idempotency, amends, and voids.

## EPIC 08 — Engine binary & CLI
- [x] `meter-api` HTTP surface: accounts (open/balance/grant/entries), reservations (reserve/settle/void), health; typed error→HTTP mapping
- [x] `meter-engine` binary: serves HTTP over Postgres, runs migrations on boot, env config (METER_DATABASE_URL/METER_LISTEN_ADDR), tracing
- [x] e2e HTTP test (open→grant→balance→reserve→settle→deny) green against real Postgres
- [x] Analytics query API on the authoritative Postgres data: `GET /v1/accounts/:id/usage-by-day?start&end` (daily credit time series, UTC-bucketed); e2e-tested
- [x] gRPC surface (proto) for control-plane RPC; **role-selectable services**: the engine serves HTTP
  and gRPC, and `METER_ROLES` (comma-separated `http`,`grpc`; default both) selects which surfaces a
  process runs, so a deployment can split them across dedicated replicas. Role parsing is unit-tested;
  gRPC-over-the-wire is e2e-tested (`grpc_server_serves_ledger_over_the_wire`).
- [ ] OpenAPI emission + typed client codegen
- [x] `meter-cli` (`meterctl`): `migrate`, `seed`, `balance`, `entries`, `grant`, `price`, `sweep`,
  `void`, `void-run`, and `reconcile` (rollup-vs-SoR, exits non-zero on drift) — env-configurable
  (`METER_DATABASE_URL` / `METER_CLICKHOUSE_URL`); the DB-touching commands are tested against real
  containers via the built binary.

## EPIC 09 — Control plane (TypeScript: Effect + Drizzle)
- [~] `apps/control-plane`: Effect HTTP API (`HttpRouter`, one module per resource) over Drizzle — health, organizations, products, notifications (raise/pull/read/ack), alert rules (create/list/enable) with `Schema`-validated bodies/query/path params and typed-error→JSON mapping (400/404/500); `Database` service (Postgres in prod, PGlite in tests); shared repository error channel; e2e-tested via in-process test server + `HttpClient` (11 tests). RLS/`withTenant`, RBAC/API keys, gRPC-to-engine pending
- [ ] Resources: orgs/teams/users/roles (RBAC), API keys, products/agents, rate-card config, budgets/grants config, webhooks, invoices
- [ ] gRPC client to engine for all money/usage ops; never computes money
- [~] API-key auth: mint (SHA-256 hashed, token shown once) / list / revoke, and a Bearer middleware (`METER_REQUIRE_AUTH`) enforced on all routes except `/health`; dashboard sends its key when configured; e2e-tested. **RBAC** done: every key carries a role (`viewer`/`member`/`admin`, ranked); the middleware enforces it by method + resource (reads → viewer, writes → member, credential management → admin), keys default to `admin` for backward-compat, migration `0005` adds the column; e2e-tested (`auth.test.ts` RBAC block, 28 control-plane tests green). **OpenAPI emission** done (EPIC 01).
**agent-doctor in CI** done: a CI job scans the control-plane's Effect-TS code (`@jgalbsss/agent-doctor`),
**99/100** after remediation — auth returns 500 (not a silent 401) on a DB error; webhook dispatch bounds
fan-out, retries only transient failures, and forwards the abort signal; time is read from the Effect
`Clock`; boundary codecs are hoisted. Sessions pending

## EPIC 10 — Invoicing
- [~] Deterministic, query-based invoice summed from the ledger (enforced==billed): `GET /v1/accounts/:id/invoice?start&end` done & e2e-tested. Hard-block-on-mismatch reconciliation pending
- [ ] State machine Draft→Grace→Finalized(immutable)→Void; sealing posting; credit-notes for corrections
- [ ] Billing periods independent of budget cycles; line items, drawdown, rev-rec view

## EPIC 11 — SDKs & integrations
Strategy: base typed clients **generated by Stainless** from the engine OpenAPI; thin hand-written
**adapters** auto-instrument the major AI clients. See `docs/SDKS.md`.
- [x] Interim TS base client + run governance (`withRun`: reserve→settle, auto-void on failure) — tested
- [x] **Hold extension** in both SDKs: optional `expiresAt`/`expires_at` hold timeout on `reserve` +
  `extendReservation`/`extend_reservation` (heartbeat to keep a long run's hold from being swept) — tested
- [x] **Token-priced two-phase enforcement** in both SDKs: `reserveUsage`/`reserve_usage` (hold sized to an
  estimated token usage, priced by the engine) + `settleUsage`/`settle_usage` (settle the actuals) — tested
- [x] **Token-priced governed run**: `withRunUsage`/`with_run_usage` wraps reserve-estimate → run →
  settle-actuals with auto-void on failure — tested (TS 18, Python 18)
- [x] **Price discovery + simulation** in both SDKs: `catalog()` (hosted model prices) and `simulate()`
  (re-rate a usage stream across two models) for cost-aware routing/budgeting — tested (TS 19, Python 19)
- [x] TS provider usage adapters (Anthropic/Claude + Agent SDK, OpenAI, Vercel AI SDK, Gemini/Vertex, Bedrock): normalize usage, `recordModelUsage`, `meteredCall`, `meterModelUsage` (price+charge via `/v1/usage`) — 13 tests
- [x] Interim Python SDK + adapters (Anthropic/Claude, OpenAI, Gemini/Vertex, Bedrock) + `meter_model_usage` + `with_run` governance — stdlib-only, 11 tests
- [ ] OpenAPI emission from the engine (code-generated via utoipa) — prerequisite, see EPIC 01
- [~] Stainless-generated base clients (TS/Python/Go/Java/Kotlin/Ruby) replacing the interim TS client;
  CI regen on spec change. **Config authored** (`sdks/stainless/openapi.stainless.yml`): targets the six
  languages, maps the engine's tags → ergonomic resources, reads the spec from `crates/meter-api/openapi.json`.
  Remaining (external): a Stainless org + `STAINLESS_TOKEN` to run generation + publish; then CI regen
  on spec change. Adapters stay hand-written per language (Stainless does not generate them).
- [x] LangChain/LangGraph adapter (`langchainUsage` / `langchain_usage`) in both SDKs, tested
- [x] First-class per-client auto-patch wrappers (monkey-patch a provider client) — **both SDKs**:
  `patchAnthropic`/`patchOpenAI` (TS) and `patch_anthropic`/`patch_openai` (Python) wrap a duck-typed
  provider client's `create` method so every call is metered automatically (charge or record mode),
  generating a per-call idempotency key, deriving the model from the response (then the request),
  fail-open via `onError`/`on_error`, and returning the provider response unchanged; each returns an
  `Unpatch` to restore the original. No dependency on the provider packages (structural typing; the
  Python side coerces dict / pydantic / plain-object usage). Tested (TS 7, Python 7).
- [x] e2e SDK tests against a running engine — **both SDKs**: opt-in suites (gated on
  `METER_E2E_BASE_URL`, skipped in the normal run) drive the full money path over the wire against a
  live engine — open → grant → reserve → settle → balance → over-budget deny → invoice, plus idempotent
  event record. A `sdks/typescript/test/e2e/run.sh` runner stands up Postgres + ClickHouse + the engine
  and runs the suite. Verified green against a real engine (isolated Postgres + ClickHouse containers).

## EPIC 12 — Hosted model rate-card catalog
- [~] `meter-ratecards`: curated, dated snapshot catalog (CATALOG_AS_OF) that builds provider-cost rate cards from a model id; Anthropic flagship models seeded; tested
- [ ] Scraper + scheduled auto-update, versioned immutable snapshots, diff-and-alert, more providers (OpenAI/Google/DeepSeek/Qwen), manual override; serve via the catalog/control-plane API

## EPIC 13 — Notifications, alerts & webhooks
- [~] Budget/alert status (read side): `GET /v1/accounts/:id/budget?...&limit` → usage vs limit + threshold status (ok/warning≥80%/exceeded≥100%); e2e-tested. Persisted alert rules + delivery below
- [x] Alert rules persisted in the control plane (create/list/enable-disable over `scope` × `metric` × `action`, `Schema`-validated) **and a budget-evaluation loop**: `POST /v1/alert-rules/evaluate` asks the engine to classify each rule's account usage vs its `creditLimit` over a window and raises a notification (+ webhook) on status escalation (engine owns the money math; control plane reacts). e2e-tested against a stubbed engine + webhook sink, with a built-in interval **scheduler**
(`METER_EVALUATION_INTERVAL_SECONDS`) that evaluates every org. Credit/spend metrics pending
- [~] Alert actions: webhook delivery shipped — signed (HMAC-SHA256 `X-Meter-Signature`), retried with backoff, event-type filtered, raised automatically when a notification is created, with an append-only delivery log; e2e-tested against a live sink. Email/in-app notify + enforce(block) pending
- [x] Notifications as first-class records: raise + pull/list (filter by status) + mark-read + ack via the control-plane API; e2e-tested. Snooze/top-up reactions pending
- [ ] Subscriptions ("notify me when …") per user + delivery preferences
- [x] Dead-letter for failed webhooks (failed deliveries recorded with attempts/error); IaC config + idempotent/async delivery queue pending

## EPIC 18 — Docs site (MDX)
- [x] Public docs site (`apps/docs`, Next.js 16 + `@next/mdx`) — Overview, Concepts (ledger / credits /
  events / reservations / leasing / budgets), full API reference (engine + control plane), SDKs,
  Self-host. Clean Dropbox-aligned aesthetic; builds to static pages; typecheck + build run in CI.
- [x] SDK pages: install/usage for TS (`@meter/sdk`) + Python (`meter-sdk`), the provider adapter
  catalog (Anthropic/OpenAI/Gemini/Bedrock/LangChain/Vercel AI), and `withRun` governance examples.
- [~] Generated API reference: **control-plane + engine done** — `/api/control-plane` and `/api/engine`
  render every path, parameter, request body, response, and schema directly from their committed OpenAPI
  documents (synced from each surface and drift-checked in CI), so neither can fall behind its contract.
  Versioned (multi-version) docs pending.
- [x] Search (Pagefind, static-export index) + static deploy target (`deploy/docs.Dockerfile` + nginx
  with clean URLs and a real 404) + a Documentation link in the dashboard sidebar (configurable via
  `NEXT_PUBLIC_DOCS_URL`)

## EPIC 17 — Audit log
- [x] Engine audit log: middleware records every mutating request (actor via `x-meter-actor`, method, path, status, time); `GET /v1/audit` lists newest-first; e2e-tested
- [x] Dashboard **Audit log** view (`/audit`): reads the engine audit endpoint, table of time/actor/method/path/status with status badges; design-system + graceful unreachable state
- [~] **Filter by actor/method/time + request-id correlation + CSV export: done** — the engine's
  `GET /v1/audit` now takes `actor`/`method`/`since`/`until` and returns `request_id`; the dashboard
  `/audit` view filters server-side (filter bar → query params), shows the request id, and exports the
  filtered rows to CSV (RFC 4180, unit-tested). Before/after diffs + control-plane-action auditing pending
- [ ] Tamper-evident (hash chain)

## EPIC 14 — Dashboard (Next.js + design system)
- [x] Scaffolded `apps/dashboard` via shadcn preset `b1z2hUjZ5c` (Next.js 16 App Router/RSC, base-ui, phosphor, Tailwind v4, Dropbox aesthetic, dark mode); bun-managed; CI runs typecheck + lint + format + **test** (vitest unit tests for the pricing-format helpers) + build
- [~] App shell (sidebar nav + org switcher) and screens wired to the control plane: Overview (stat cards + recent notifications), Organizations (+create), Products (list + create), Notifications (pull/filter/read/ack via server actions + toasts), Alert rules (create with scope/metric/action selects, enable/disable, evaluate-now), Webhooks (register + endpoints + delivery dead-letter log, enable/disable), API keys (mint with one-time token reveal + **RBAC role select**, list with role badge, revoke) — full CRUD via dialog forms + server actions. **Usage analytics** (org usage-by-model bar chart + table, per-account daily credit burn), **Events explorer** (per-account event list with status/run/properties), **Accounts** (balance: available/settled/held + immutable ledger-entry table with typed badges), **Invoices** (month-to-date statement: total credits + entries + daily breakdown), and **Audit log** view read from the engine. The Events explorer has **amend** (JSON-properties editor → engine `amend`) and **void-run** (confirm dialog → engine `void_run`) actions — both server actions behind `requireSession`. Overview shows a top-models-by-spend summary. **Rate-card catalog viewer done** (`/rate-cards` reads the
engine's hosted `/v1/catalog` — provider cost per 1M tokens, exact decimal→per-million via string math,
no floats), plus a **pricing simulator** (`/simulate`) that re-rates a usage profile across two
catalogued models via the engine's `/v1/simulate`. Custom rate-card + budgets/grants **config-write**
screens pending (need control-plane config resources)
- [~] Typed control-plane fetch client (degrades gracefully when the control plane is down) + engine read-client for analytics. **Session auth**: HMAC-signed cookie, password login (`DASHBOARD_PASSWORD`/`DASHBOARD_SESSION_SECRET`), layout-gated pages + `requireSession()` on every server action (closed the react-doctor unauthenticated-server-action finding). **Usage analytics page** (recharts, code-split via `next/dynamic`): org-scoped **usage-by-model** (bar chart + table of events/tokens/credits, from the engine's ClickHouse query API) plus per-account **daily credit burn** (usage-by-day time series). transitions.dev installed. **react-doctor** wired: `doctor` script + advisory PR workflow (`.github/workflows/react-doctor.yml`, scoped to `apps/dashboard`); score 21→54, remaining findings are vendored shadcn primitives + 2 documented auth-boundary exceptions. **Control-plane client types generated from the OpenAPI spec** (`openapi-typescript` → `control-plane.gen.ts`; CI fails on drift) — no hand-mirrored types. Lighthouse budgets pending

## EPIC 15 — Deployment & self-host
- [x] Engine Docker image (multi-stage, rustls, slim runtime) + control-plane image (tsx, boot
  migrations) + **dashboard image** (Next.js standalone on bun; built **and run-verified** —
  `/login` serves) + full production docker-compose (Postgres + ClickHouse + engine + control-plane +
  dashboard), `docker compose config` valid.
- [x] **Helm chart** (`deploy/helm/meter`): engine + control-plane + **dashboard** Deployments
  (stateless engine scales via `engine.replicas`; dashboard toggleable) + Postgres/ClickHouse
  StatefulSets (toggleable for external managed stores) + credentials Secret; liveness on `/health`,
  readiness on `/health/ready` (engine + control plane ping their stores; dashboard on `/login`);
  optional `controlPlaneHost`/`engineHost` ingress; `helm lint` clean, renders for default /
  external-store / HA / dashboard-disabled value sets. Migrations run on boot (no separate job).
- [x] **Published images**: `release.yml` builds + pushes the engine, control-plane, and dashboard
  images to `ghcr.io/<owner>/meter-*` on a `v*` tag (semver + sha + latest tags, buildx + gha cache).
- [x] **Helm Ingress/TLS**: optional, gated `ingress.enabled` template routing a host to the dashboard
  console, with className + TLS secret; renders clean (disabled by default).
- [~] **Expose control-plane/engine via Ingress: done** — optional `ingress.controlPlaneHost` /
  `ingress.engineHost` route each HTTP API on its own host (TLS covers every set host); `helm lint` +
  multi-host render verified. **Private-VPC notes: done** — air-gapped self-host section (internal
  ingress, external stores, image mirroring, snapshot-catalog = no egress, secrets). `meter migrate`
  job option pending (needs a migrate entrypoint in a published image).
- [ ] Single-tenant & multi-tenant modes; opt-in scale-out backends behind traits

## EPIC 16 — Cross-cutting: security, observability, e2e, benchmarks
- [~] RBAC + tenant isolation tests; secrets handling. **App-level tenant isolation done** per
  **[ADR 0007](../docs/adr/0007-tenant-isolation.md) (accepted)**: api-key `scope` (platform vs org;
  migration 0006 backfills existing keys to platform, new keys org), the auth middleware publishes the
  principal, every org-scoped route authorizes the target org against the caller (cross-org read/create
  → 403), by-id mutations are org-scoped via `byIdInOrg` (cross-org id → 404), org CRUD is platform-only,
  and minting a platform key requires a platform caller. Proven by `test/tenant-isolation.test.ts`. The
  original gap (a key for org A reading org B via `?orgId=`) is closed. **RLS defense-in-depth pending**
  (EPIC 02) — needs per-request `SET LOCAL` plumbing, a non-owner DB role, and a real-Postgres test
  (PGlite doesn't enforce RLS).
- [x] **Dependency audit + CI gate (TS).** All high/critical advisories fixed: drizzle-orm
  0.38→**0.45.2** (runtime), vitest 2→**3.2.6** + vite→**6.4.3** (dev) across control-plane + SDK —
  every suite green (27 + 15), typecheck clean, `db:generate` no drift. `pnpm audit` 8→1 (the lone
  remaining moderate is `esbuild` deep inside drizzle-kit's deprecated bundler chain, dev-only). A
  `pnpm audit --audit-level high` gate now runs in CI. (`cargo audit` for Rust still to add.)
- [~] Observability: control-plane **request-id + structured access-log middleware** (propagates/echoes
  `x-request-id`, annotates per-request logs, logs method/path/status/duration); tested. Engine tracing
  + metrics export pending.
- [~] **Full e2e**: SDK → engine → ledger → control plane → invoice, against real stores via compose
  (`deploy/e2e/smoke.sh` + `flow.py`, driving the engine flow through the Python SDK). Harness built and
  run; it found + fixed a `protoc` gap that broke the engine image and CI rust build. **Open blocker
  (engine-side):** the engine's ClickHouse client (`meter-store-ch`) sends no credentials, but CH 24.8
  rejects passwordless remote auth — the engine crash-loops on boot, so the flow can't complete until
  the engine supports CH credentials (see `deploy/e2e/README.md`). Dashboard leg still to add.
- [~] Criterion benchmarks: pricing hot path (`cargo bench -p meter-pricing`: `cost` ≈135 ns, `price_usage` ≈158 ns/event) and the **Postgres-backed enforcement** hot path (`cargo bench -p meter-store-pg`: `reserve`+`settle` ≈0.95 ms/call vs a local container — representative, indexed idempotency, no O(n) growth).
- [x] Concurrent **load harness**: 8 workers × 25 reserve→settle cycles in parallel against one funded account proves exact credit conservation under contention (settled == funded − Σactuals, held == 0) + reports throughput. Published SLO results vs SLO.md pending
- [x] **Event-ingest throughput harness** (`meter-store-ch/tests/throughput.rs`): single-record baseline vs `record_batch` across a batch-size × concurrency sweep against a real ClickHouse container, plus usage-read latency at scale and an exactly-once replay check. Numbers + targets published in `SLO.md` (ingest ~1M events/s; usage reads single-digit ms, O(rollup groups)).
- [x] DCO, CONTRIBUTING (refreshed for control plane + dashboard), SECURITY, issue forms + PR template; README/docs kept current every epic

## EPIC 19 — Provider-scale throughput (ADR 0005)
Target: OpenAI/Anthropic volume — millions of metering ops/sec, billions of events/day — without
trading away the sacred ledger. The bottleneck is the transactional money path; we attack it behind the
`LedgerBackend` seam so the rest of the system is untouched (simplicity).
- [x] **Per-session leasing** — spend locally, one ledger round-trip per session not per token; spreads
  hot-account contention across per-session rows. Done (EPIC 03), conformance-tested.
- [ ] **TigerBeetle `LedgerBackend`** (`meter-store-tb`, `tigerbeetle-unofficial`): two-phase transfers
  = reserve/settle/void, integer credits (`credit × 10^scale`, u128), no-overdraft via
  `debits_must_not_exceed_credits`; grants/charges/transfers per the ADR 0005 mapping. Must pass the
  **shared `meter_ledger::conformance` suite** (no-overdraft + idempotency) against a real TB server
  (testcontainers), exactly like the in-memory + Postgres backends.
- [~] **Firehose at scale**: batched event ingest shipped — `EventStore::record_batch` (one multi-row
  ClickHouse insert) with content-addressed, read-free idempotency (event id = UUIDv5 of
  `(org_id, idempotency_key)`); exposed as `POST /v1/events/batch` (202). A `usage_rollup` materialized
  view keeps usage reads sub-linear. Measured single-node ~1.0–1.1M events/s (Append) / ~0.68M
  (ExactlyOnce dedup); single-record baseline was ~1.1k/s. `IngestMode` lever (`METER_INGEST_MODE`)
  picks dedup-on-write vs upstream EOS. Optional Redpanda/Kafka buffer behind `IngestSource` pending.
- [ ] **Horizontal scale**: stateless engine replicas; ClickHouse cluster; per-org sharding of the
  money store as the final lever.
- [~] **Throughput SLO gates**: event-ingest throughput harness (`meter-store-ch/tests/throughput.rs`)
  drives a real ClickHouse container, sweeps batch-size × concurrency, reports events/sec for both
  ingest modes + read latency, and **proves exactly-once by replaying the whole load**. Targets +
  measured numbers published in `SLO.md`. Ledger ops/sec (shared vs leased vs TigerBeetle) + CI
  regression gate still pending.
