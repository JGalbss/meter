# meter documentation

The map of everything written about meter. Start with the [README](../README.md) for what meter is and
a copy-paste quickstart; this index points to the rest.

There are two homes for docs. The Markdown in this `docs/` folder is the **design record** — the why and
the how, versioned next to the code. The [public docs site](../apps/docs) (`apps/docs`, served on
`:3001`) is the **user guide** — concepts, API reference, SDKs, and self-host, with search. They link to
each other and don't duplicate.

## Start here

- [README](../README.md) — what meter is, the quickstart, and what works today.
- [VISION.md](VISION.md) — the problem meter solves and the product thesis.
- Public docs site `/quickstart` — run meter end to end in ten minutes.

## Design and decisions

- [ARCHITECTURE.md](ARCHITECTURE.md) — the system design baseline: the ledger, pricing, enforcement, and
  the scale-out path. Amended over time by numbered ADRs (a banner at the top points to the changes).
- [DECISIONS.md](DECISIONS.md) — the decision log at a glance, with the ADR that amended each.
- [adr/](adr/) — the Architecture Decision Records (the engine/control-plane split, events in
  ClickHouse, wire-protocol versioning, tenant isolation, and the rest).

## Reference

- [SDKS.md](SDKS.md) — the SDK strategy and the provider adapters.
- [SLO.md](SLO.md) — the performance and reliability contract.
- [BENCHMARKS.md](BENCHMARKS.md) — measured hot-path numbers, set honestly against comparable systems.
- Per-component READMEs: the engine crates ([crates/](../crates/)), the
  [control plane](../apps/control-plane/README.md), the [dashboard](../apps/dashboard/README.md), the
  [docs site](../apps/docs/README.md), and the [TypeScript](../sdks/typescript/README.md) and
  [Python](../sdks/python/README.md) SDKs.
- [proto/](../proto/README.md) — the engine ⇄ control-plane protobuf contract.

## Operate

- Public docs site `/self-host` — Docker Compose, Helm, configuration, and air-gapped deployment.
- [deploy/helm/meter/README.md](../deploy/helm/meter/README.md) — the Helm chart.
- [deploy/e2e/README.md](../deploy/e2e/README.md) — the cross-stack smoke test.

## Contribute

- [CONTRIBUTING.md](../CONTRIBUTING.md) — build, test, lint, and the engineering standards.
- [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md) — how we work together.
- [SECURITY.md](../SECURITY.md) — reporting a vulnerability.
- [ROADMAP.md](../ROADMAP.md) — shipped, in progress, and planned.
- [tickets/](../tickets/README.md) — the living build checklist (the detail behind the roadmap).
