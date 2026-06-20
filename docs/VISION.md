# meter — Product Vision

> Status: living document. This is the **why** and the **what**. The architecture (the **how**)
> lives in `docs/ARCHITECTURE.md`. Distilled from the founder's kickoff brief — captured verbatim
> in intent so the product thesis survives as the implementation evolves.

## 1. The opportunity

The agent economy has created a new, urgent metering problem:

- **Metronome** (acquired by Stripe, ~$1B) is the incumbent usage-metering/billing ledger, but the
  product is hard to use — the mental model around **rate cards, rate-card "aliases"/subsets, and
  products** is confusing. The good idea worth keeping is that **it is a ledger**.
- **Orb** and **Paid.ai** are newer entrants doing usage-based / agent billing.
- **Lago** is the open-source reference point — useful for concrete schema/engine inspiration.
- Simultaneously there is a **massive cost-optimization wave**: teams are juggling frontier models
  vs. cheaper/open/Chinese models, navigating large price spreads, and trying to **track agent costs
  across many dimensions** in real time. They need to meter, attribute, budget, and bill — cleanly.

**Thesis:** Build the metering + billing + invoicing engine the agent era actually needs — a
**ledger-first**, **clean**, **extremely high-performance**, **open-source and self-hostable** system
that an **agent can run on**, and that **humans can build with**.

## 2. Product principles

1. **Ledger-first.** An immutable, auditable, double-entry-style ledger is the heart of the system.
   Everything reconciles back to one source of truth. No mutable balances that drift.
2. **Cleaner than Metronome.** One coherent, learnable conceptual model. Kill the rate-card/sub-card
   confusion. A developer should grok it in minutes.
3. **Agent-native, human-usable.** Designed so an agent's runtime can meter itself and be governed in
   real time; designed so humans can configure, price, budget, and invoice with confidence.
4. **Performance is a feature.** Possibly millions of usage events; very fast reads; high TPS;
   sub-millisecond-class enforcement on the hot path. This forces a serious engine and storage design
   (sharding, OLAP, specialized ledger).
5. **Flexibility without sprawl.** Customers can price/meter however they want (tokens, credits,
   artifacts, outcomes) — but the primitives stay few and composable.
6. **Open source + hosted.** Self-hostable (single-tenant and multi-tenant), Docker in a private VPC;
   we also run a hosted version.
7. **No shortcuts. Built for the long term.** Correct DB schemas, real migrations, heavily testable,
   clean and performant code. We engineer this from the bottom up.

## 3. Core capabilities (scope)

### Metering
- Ingest usage events at very high throughput with **idempotency / exactly-once** semantics.
- Meter by **tokens**, by **credits**, by **artifacts/outputs**, and by **agent outcomes** (things
  the agent actually accomplished), not just raw tokens.
- Meter per **product** and per **agent** (a customer may run many agents/products).

### Hierarchy & attribution
- Rich hierarchy: **org / tenant → teams → users → roles (RBAC)**.
- Attribute and aggregate usage and credit consumption across any level of the hierarchy, with fast
  complex queries (e.g. "credit burn by team, by product, this week").

### Pricing & rate cards
- **Multi-dimensional rate cards**, especially for models: input / output / cache-read / cache-write,
  long-context vs short-context tiers, reasoning tokens, tool/web surcharges, batch discounts —
  modeled cleanly per model.
- **Translation layers**: token → credit; credit pegged to a fixed cash value (e.g. 2¢); volume
  tiers; **margin** baked in. Sell credits in packs/levels.
- Customers choose their level of abstraction (charge per token, per credit, per outcome) — the
  engine handles the translation.
- **Hosted rate-card catalog**: we scrape/maintain up-to-date provider model prices so customers
  don't have to. Bring-your-own rate cards also supported. (Self-serve "use our model rate cards.")

### Budgets, grants & billing
- **Budgets** per user / team / org. **Credit grants / allotments.** Free promo credits to drive
  usage.
- **Periodic budgets are SEPARATE from invoicing/billing periods.** Example: a company prepays a
  credit grant, is invoiced biannually, while an admin configures "this product: 400 credits/week per
  user." Burn-down, priority of burn (free vs paid vs granted), and sensible defaults.
- **Invoices & billing periods** with their own cadence, independent of periodic budget windows.

### Real-time enforcement (hot path)
- Block / deny an agent that is out of budget or credits, with very low latency and **no overdraft**.
- **Reserve → settle** (two-phase): reserve estimated cost before the LLM call, settle with actuals
  after. Graceful fail-open/fail-closed policy.

### Notifications & integrations
- Emails, in-app notifications, and **webhooks** (configurable URL + scopes).
- Configurable via IaC / Terraform-style admin permissions.

### SDKs
- Drop-in SDKs that **wrap agent/LLM calls like OpenTelemetry / Datadog spans** and auto-emit usage.
- Auto-read provider usage (tokens, cache, etc.) and emit to the platform; integrate with the
  reserve/settle enforcement API. TS + Python first.

## 4. Deployment & business model

- **Open source** core, **hosted** commercial offering.
- **Self-host**: single-tenant and multi-tenant; Docker container in a customer's private VPC,
  exposing a port (proxy/ingress in front — to be designed); plus our managed cloud.
- Licensing to protect the hosted business is an explicit open decision (see ARCHITECTURE.md).

## 5. Differentiation

- **Cleaner conceptual model** than Metronome (the #1 complaint we're solving).
- **Agent-first**: outcome/artifact metering, real-time hot-path governance, OTel-style auto-instrumentation.
- **Performance ceiling** far above CRUD-billing tools.
- **Open source + self-hostable** in the customer's VPC (data-residency / trust), unlike closed SaaS.
- **Batteries-included model rate-card catalog** kept current for the whole industry.

## 6. Non-negotiables

- Correct, auditable financial ledger (no lost/double-counted/overspent credits).
- High ingest throughput + fast reads + low-latency enforcement.
- Heavily testable; clean, performant code.
- Correct DB schemas and a real migration strategy from day one.

## 7. Open questions (to resolve in design)

These are surfaced for an explicit founder decision; the ADR will carry recommended defaults.

- Ledger store: specialized (TigerBeetle) vs Postgres-native vs hybrid?
- Analytics store: ClickHouse vs alternatives? Required for v1 or scale-out later?
- Ingest log: Kafka/Redpanda/NATS/Redis Streams — or start synchronous?
- Engine language: Rust vs Zig (founder leaning); where the TS control-plane ends and the engine begins.
- Self-host default footprint: how few stateful systems can we require?
- Licensing: Apache-2.0 vs AGPL-3.0 vs BSL/Elastic v2.
