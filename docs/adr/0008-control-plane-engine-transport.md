# ADR 0008 — Control-plane → engine runtime transport is gRPC (proto)

Status: accepted; implementation tracked in `/tickets` (EPIC 01, EPIC 12)

Resolves an open question left by ADR 0001 (the protobuf engine⇄control-plane seam) and the EPIC 01
contracts work: *what wire protocol does the control plane actually use at runtime to call the engine?*

## Context

ADR 0001 established that money-truth lives only in the engine and the control plane calls the engine
for it — over a protobuf seam (`proto/meter/v1`). The contract exists and is enforced: the Buf module is
linted and breaking-change gated in CI, and `meter-proto` generates the Rust `prost`/`tonic` types and
service stubs the engine serves (`LedgerService`, `IngestService`, `QueryService`, `ConfigService`).

The **TypeScript side of that contract was never wired**. The control plane today makes exactly one
engine call — budget classification — and it does so over the engine's **REST/OpenAPI** surface
(`GET /v1/accounts/{id}/budget`) via a hand-written `fetch` plus a hand-written `Schema` decoder
(`apps/control-plane/src/engine/client.ts`). That works, but it is the one place in the system where a
wire shape is hand-mirrored rather than generated, and it diverges from the stated architecture. As the
control plane grows to call more engine services (invoice reads, rate-card/budget config push via
`ConfigService`, grants via `LedgerService`), the number of hand-mirrored calls — and the drift risk —
grows with it.

Two coherent end-states were considered:

- **Keep REST.** The engine's REST surface is first-class (it has an OpenAPI contract and is what the
  SDKs use). Simplest; no engine-side work. But it cements a hand-mirrored seam and contradicts ADR 0001.
- **Use gRPC (proto).** One typed contract, generated on both sides, no hand-mirrored types — the design
  ADR 0001 always intended. Requires generating the TypeScript client (`ts-proto`/connect) and an
  engine-side gRPC method for budget classification, which does not yet exist on `QueryService`.

## Decision

The engine⇄control-plane **runtime transport is gRPC**, generated from `proto/meter/v1` on both sides —
`prost`/`tonic` for the engine, `ts-proto`/connect for the control plane. The hand-written REST client
is transitional and will be removed.

Concretely:

1. **Budget classification becomes a gRPC method.** A read-side `QueryService.BudgetStatus(BudgetStatusRequest)
   returns (BudgetStatusResponse)` is added to `query.proto` and implemented by the engine, alongside the
   existing read-side query RPCs. (The `Budget` message in `config.proto` is the *write*-side `SetBudget`;
   this is the *read*-side status: used/limit/remaining/ratio/class.) This is an additive, non-breaking
   proto change — but adding the RPC makes the engine's `QueryService` impl require the new method, so the
   **engine implements the RPC and the proto change in the same change** (the control-plane client and the
   proto RPC cannot land first without breaking the engine build).
2. **The control plane consumes a generated client.** `ts-proto`/connect codegen produces the TypeScript
   client from the same Buf module; `engine/client.ts` is rewritten to call `QueryService.BudgetStatus`
   through it, deleting the hand-written `fetch` + `Schema` decoder. Boundary decoding is the generated
   message type, not a hand-mirrored schema.
3. **Interim.** Until the engine serves `BudgetStatus` over gRPC, the control plane keeps using the REST
   call — explicitly transitional, not a second supported transport. No new hand-mirrored engine calls
   are added in the interim; new engine integrations wait for the generated client.

The engine continues to expose its REST/OpenAPI surface — it is the public, SDK-facing API and is not
affected by this decision. This ADR governs only the **control-plane → engine** internal call path.

## Consequences

- One contract, generated on both sides: no hand-mirrored engine wire types in the control plane; a
  breaking change is caught by `buf breaking` (per ADR 0006) rather than surfacing as a runtime decode error.
- A cross-component dependency: the gRPC migration completes only when the engine ships the `BudgetStatus`
  RPC. Sequencing is engine-first (proto RPC + handler together), then control-plane client generation +
  call migration. Tracked in EPIC 01 (codegen) and EPIC 12 (the engine RPC).
- A new build step in the control plane (`ts-proto`/connect codegen) and a gRPC transport dependency for
  Node → `tonic` (connect-es over HTTP/2, or an equivalent gRPC client). Added when the client is wired,
  not before, to avoid unused scaffolding.
- The hand-written `engine/client.ts` REST path is deleted once the migration lands, removing the system's
  only hand-mirrored wire contract.
