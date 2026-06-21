# meter-store-pg

The PostgreSQL ledger backend — meter's money-truth store. `PgLedger` implements
`meter_ledger::LedgerBackend` over `sqlx`: the same append-only double-entry ledger the in-memory
reference defines, made durable.

## What's inside

- `PgLedger` — the backend. Wrap a pool with `PgLedger::new(pool)`. Per-account settled balances live
  on `ledger_accounts` and update transactionally; reserve and settle serialize on the account row with
  `SELECT … FOR UPDATE`, which is what makes a HARD limit unable to overdraft under concurrency.
- `PgLedger::migrate()` — applies the engine ledger migrations from `./migrations`. It refuses on
  version skew: if the database carries a migration this binary does not ship (a newer build already
  migrated ahead), the run fails rather than operating against an unknown schema.
- `PgLedger::ping()` — readiness probe; confirms the database is reachable and answering.
- `PgConfig`, `BudgetRecord`, `RateCardRecord` — config rows read alongside the ledger.
- `DayUsage`, `PeriodUsage` — usage-report shapes for the invoice and budget endpoints.

## Where it sits

Depends on `meter-core` (Money/Credit/ids) and `meter-ledger` (the trait it implements). The engine
binary (`meter-engine`) and `meter-api` build on it; `meterctl` runs its migrations.

## Verification

`PgLedger` runs the shared `meter_ledger::conformance` suite against a real Postgres container
(`testcontainers-modules`), with the in-memory reference as the oracle — the no-overdraft and
idempotency invariants hold here exactly as they do in memory. `cargo bench -p meter-store-pg`
measures the durable reserve → settle path.
