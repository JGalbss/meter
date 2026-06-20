# Architecture Decision Records

`ARCHITECTURE.md` is the locked baseline. As the design evolves, each material change is recorded here
as a small, numbered, append-only ADR rather than by rewriting the baseline — so the *reasoning* and
the *history* survive. Format: Context → Decision → Consequences. An ADR may amend or supersede a
section of `ARCHITECTURE.md`; it says so explicitly at the top.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-engine-controlplane-split.md) | Engine / control-plane split and the protobuf seam | Accepted |
| [0002](0002-editable-events-and-run-voiding.md) | Editable events, custom fields, and one-call run voiding | Accepted |
