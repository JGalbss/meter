# meter-cli

`meterctl` — the admin CLI for the meter engine. It runs migrations and operational tasks directly
against the engine stores **without booting the HTTP/gRPC server**, so it fits cron jobs, deploy hooks,
and incident response.

Money-truth lives in Postgres (the ledger); events + analytics live in ClickHouse (ADR 0001/0003).
Commands connect to whichever store they need.

## Configuration

The ledger commands take a Postgres URL; the rollup commands (`reconcile`, `rebuild-rollups`) take a
ClickHouse URL. Both default to environment variables, so in a configured deployment you can drop the
flags. `price` needs neither — it prices in memory.

- `--database-url` / `METER_DATABASE_URL` — Postgres (the ledger).
- `--clickhouse-url` / `METER_CLICKHOUSE_URL` — ClickHouse (events + rollups).

A flag always overrides the environment variable. `seed` needs its Postgres URL from `--database-url`
or `METER_DATABASE_URL`; without either it exits.

## Commands

| Command | What it does |
| --- | --- |
| `migrate` | Apply the ledger migrations (idempotent; refuses on version skew). |
| `seed [--credits N]` | Open a funded org account for local dev (migrates first). `--credits` defaults to `1000000`. |
| `balance --account <uuid>` | Print an account's settled / held / available balance. |
| `entries --account <uuid>` | List an account's immutable ledger entries (the audit trail). |
| `grant --account <uuid> --credits N` | Grant credits to an existing account. |
| `price --model <id> [--input/--cache-read/--cache-write/--output N]` | Price token usage for a catalog model (no database) — cost in USD and credits. |
| `sweep` | Release expired open holds (auto-void for stranded reservations). |
| `void --reservation <uuid>` | Release one specific open hold (e.g. a stuck hold from a crashed run). |
| `void-run --run <uuid>` | Reverse a whole run's ledger impact: release its holds, refund its settled charges (idempotent). |
| `reconcile --org <uuid>` | Reconcile the pre-aggregated usage rollups (by model and by promoted field) against the event source of record. Prints any drift and exits non-zero when it finds some, so it can gate a cron/alert. |
| `rebuild-rollups --org <uuid>` | Repair drift: clear and repopulate an org's rollups from the event source of record. |

## Examples

```bash
# Point the ledger commands at Postgres once (a flag would override this).
export METER_DATABASE_URL="postgres://meter:meter@localhost:5432/meter"

# Bring a fresh database up and seed a funded account (1,000,000 credits by default).
meterctl migrate
meterctl seed --credits 1000000

# Inspect an account.
meterctl balance --account 11111111-1111-1111-1111-111111111111
meterctl entries --account 11111111-1111-1111-1111-111111111111

# Price a call without touching the database.
meterctl price --model claude-opus-4-8 --input 1000 --output 500
```

```bash
# Nightly drift check against the event source of record, then repair. These read ClickHouse, not
# Postgres — a non-zero exit from reconcile fails the job and triggers the rebuild.
export METER_CLICKHOUSE_URL="http://localhost:8123"
meterctl reconcile --org 11111111-1111-1111-1111-111111111111 \
  || meterctl rebuild-rollups --org 11111111-1111-1111-1111-111111111111
```

The store-touching commands are integration-tested against real Postgres and ClickHouse containers via
the built binary.
