# Architecture Decision Records

`ARCHITECTURE.md` is the v1 baseline. As the design evolves, each material change is recorded here as a
small, numbered, append-only ADR rather than by rewriting the baseline — so the *reasoning* and the
*history* both survive. Format: Context → Decision → Consequences. An ADR may amend or supersede a
section of `ARCHITECTURE.md`; it says so explicitly at the top, and the baseline carries a banner
pointing back to the ADRs that have changed it. Where a baseline section and an ADR conflict, the ADR
wins.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-engine-controlplane-split.md) | Engine / control-plane split and the protobuf seam | Accepted |
| [0002](0002-editable-events-and-run-voiding.md) | Editable events, custom fields, and one-call run voiding | Accepted |
| [0003](0003-events-in-clickhouse.md) | Events live in ClickHouse, not Postgres | Accepted |
| [0004](0004-audit-log-in-clickhouse.md) | Audit log lives in ClickHouse, not the money database | Accepted |
| [0005](0005-provider-scale-throughput.md) | Scaling to provider-grade throughput | Accepted |
| [0006](0006-wire-protocol-versioning.md) | Wire-protocol versioning policy (proto + OpenAPI) | Accepted |
| [0007](0007-tenant-isolation.md) | Control-plane tenant isolation (platform vs org-scoped keys) | Accepted |
| [0008](0008-control-plane-engine-transport.md) | Control-plane → engine runtime transport is gRPC (proto) | Accepted |
| [0009](0009-amend-delta-posting.md) | Event amendment → ledger delta posting | Accepted |
