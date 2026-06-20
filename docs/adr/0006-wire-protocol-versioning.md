# ADR 0006 — Wire-protocol versioning policy

Status: accepted (policy); enforcement tracked in `/tickets` (EPIC 01)

Extends DECISIONS #6 (versioned OpenAPI contract) and ADR 0001 (the protobuf engine⇄control-plane
seam) with the concrete rules and CI gates for how the two wire contracts evolve.

## Context

meter has two wire contracts, each with consumers we do not control:

- **engine ⇄ control-plane** — protobuf / gRPC (`proto/meter/v1`), codegen `prost` (Rust) and
  `ts-proto`/connect (TypeScript control plane).
- **control-plane ⇄ dashboard / customer** — OpenAPI 3.1, emitted from the same Effect `Schema`s that
  validate requests, codegen `openapi-typescript` (dashboard) and Stainless (published SDKs).

Self-hosters pin SDK and image versions; the dashboard client is generated; third parties call the REST
API directly. A change that silently breaks a field shape, an enum, or an endpoint breaks all of them at
once. We need one policy for evolving these surfaces — and gates that make an accidental break
impossible to merge, so a breaking change can only ever be a deliberate, declared act.

## Decision

### 1. Versioning scheme

- **gRPC/proto**: the major version lives in the package and path — `meter.v1`. A breaking change
  requires a **new major package** (`meter.v2`) served *alongside* `v1` until `v1` is retired. Within a
  major, only backward-compatible changes are allowed.
- **REST/OpenAPI**: the major version lives in the URL path — `/v1/...` — and `info.version` carries
  the spec's semver. Same additive-only rule within `/v1`; a breaking change is a new `/v2` path tree
  served alongside `/v1`.

### 2. Compatibility rules within a major (both surfaces)

- **Additive only**: new optional fields, new endpoints/RPCs, new messages/services, new enum values.
- **Never**: remove or rename a field, change its type or cardinality, tighten validation on an existing
  field, repurpose a field number, change an enum value's meaning, or make an optional field required.
- **Field numbers are immutable.** Removed proto fields are `reserved` (both number and name); numbers
  are never reused or renumbered.
- **Unknown-tolerance both ways.** proto ignores unknown fields by design; JSON/REST consumers must
  ignore unknown properties (generated clients do). Consumers handle unknown enum values via a
  default/unknown arm — and the engine, per the Rust rules, never hides new variants behind a catch-all
  `_`.

### 3. Enforcement — the teeth (CI gates)

- **proto**: `buf lint` (STANDARD) enforces naming/structure; **`buf breaking --against` main** blocks
  any wire-incompatible change on every PR (the `proto` CI job). A breaking proto edit cannot merge into
  a major; it forces a new package.
- **OpenAPI**: the spec is **generated** from the Effect `Schema`s that also validate requests — there
  is no hand-mirrored type to drift. A checked-in `openapi.json` plus a **freshness test** fails if the
  served document diverges from the committed artifact, and the dashboard client is regenerated with a
  **drift gate** (`gen:api` → `control-plane.gen.ts`, CI fails on diff). A REST breaking-change gate
  (OpenAPI diff, mirroring `buf breaking`) is the one remaining piece — tracked in EPIC 01; until it
  lands, REST breaking-prevention rests on Schema-first generation plus review.

### 4. Deprecation lifecycle

- Mark deprecated before removing: proto `[deprecated = true]`; OpenAPI `deprecated: true` plus a
  `Deprecation` and `Sunset` response header on the affected operations.
- A deprecated element keeps working for a published sunset window (≥ one minor cycle) before it is
  removed.
- **Removal happens only on a major bump.** The previous major is served in parallel until its sunset
  date, then retired.

### 5. SDK & codegen alignment

- Generated clients (`ts-proto`/connect, `openapi-typescript`, Stainless) are pinned to a contract
  major and regenerated in CI; a contract change not reflected in the generated client fails the drift
  gate.
- An SDK's semver tracks the contract major it targets; bumping the contract major is a major SDK
  release.

## Consequences

- Consumers can pin and upgrade with confidence: a silent break is unmergeable (the gates), and a real
  break is only possible as an explicit new major served in parallel.
- The cost is running two majors side by side for a sunset window — more surface to maintain — which is
  the deliberate price of never breaking a pinned client.
- One gap remains: the REST breaking-diff gate is not yet wired (only freshness + client drift are).
  Until it lands, the Schema-first single-source generation makes REST drift structurally unlikely, and
  review covers the rest. Tracked in EPIC 01.
