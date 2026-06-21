# meter-cli

`meterctl` — the admin CLI for the meter engine. It runs migrations and operational tasks directly
against the engine stores **without booting the HTTP/gRPC server**, so it fits cron jobs, deploy hooks,
and incident response.

Money-truth lives in Postgres (the ledger); events + analytics live in ClickHouse (ADR 0001/0003).
Commands connect to whichever store they need.

## Configuration

Most commands take a Postgres URL; `reconcile` takes a ClickHouse URL. Both default to environment
variables, so in a configured deployment you can omit the flags:

- `--database-url` / `METER_DATABASE_URL` — Postgres (the ledger).
- `--clickhouse-url` / `METER_CLICKHOUSE_URL` — ClickHouse (events + rollups).

## Commands

| Command | What it does |
| --- | --- |
| `migrate` | Apply the ledger migrations (idempotent; refuses on version skew). |
| `seed --credits N` | Open a funded org account for local dev (migrates first). |
| `balance --account <uuid>` | Print an account's settled / held / available balance. |
| `entries --account <uuid>` | List an account's immutable ledger entries (the audit trail). |
| `grant --account <uuid> --credits N` | Grant credits to an existing account. |
| `price --model <id> [--input/--cache-read/--cache-write/--output N]` | Price token usage for a catalog model (no database) — cost in USD and credits. |
| `sweep` | Release expired open holds (auto-void for stranded reservations). |
| `void --reservation <uuid>` | Release one specific open hold (e.g. a stuck hold from a crashed run). |
| `void-run --run <uuid>` | Reverse a whole run's ledger impact: release its holds, refund its settled charges (idempotent). |
| `reconcile --org <uuid>` | Reconcile the pre-aggregated usage rollup against the event source of record, per model. Prints any drift and exits non-zero when it finds some, so it can gate a cron/alert. |

## Examples

```bash
# Bring a fresh database up and seed a funded account.
meterctl migrate --database-url "$METER_DATABASE_URL"
meterctl seed --credits 1000000

# Inspect an account.
meterctl balance --account 11111111-1111-1111-1111-111111111111
meterctl entries --account 11111111-1111-1111-1111-111111111111

# Price a call without touching the database.
meterctl price --model claude-opus-4-8 --input 1000 --output 500

# Nightly drift check (non-zero exit fails the job if the rollup diverged).
meterctl reconcile --org 11111111-1111-1111-1111-111111111111
```

The DB-touching commands are integration-tested against real Postgres/ClickHouse containers via the
built binary.
