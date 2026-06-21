# meter-core

The domain primitives every other engine crate is built on: strongly-typed identifiers, exact-decimal
money, and the credit unit of account. It owns no IO, no storage, and no async — just the value types.

## What's inside

| Type | What it is |
|---|---|
| `Money` | An exact amount with an explicit `Currency`. Backed by `rust_decimal::Decimal`, never a float. Cross-currency arithmetic is a typed `MoneyError`, never a silent mix. |
| `Credit` | The tenant-defined unit of account, decoupled from any currency and pegged to a cash value in pricing config. Also an exact decimal; can be fractional. |
| `Id<T>` | A `UUIDv7` parameterised by a zero-sized marker, so an `OrgId` can never be passed where a `UserId` is expected. `Id::deterministic` derives a stable `UUIDv5` for content-addressed idempotency. |
| typed ids | `AccountId`, `OrgId`, `EventId`, `RunId`, `EntryId`, `RateCardId`, and the rest — all `Id<T>` aliases. |

`Money` is the only place currency math happens, and it returns `Result<_, MoneyError>` on mismatch.
`Credit` and `Money` both serialize as decimal **strings** so no float ever rounds the amount on the
wire.

## Where it sits

The root of the workspace dependency graph: `meter-core` depends on nothing internal, and every other
engine crate depends on it.

Edition 2021, `#![forbid(unsafe_code)]`. Run the unit tests with:

```bash
cargo test -p meter-core
```
