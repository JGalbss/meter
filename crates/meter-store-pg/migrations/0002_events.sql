-- Usage events: immutable facts with arbitrary custom fields, append-only amendments, and run-level
-- voiding. account_id is a logical reference (not a hard FK to ledger_accounts) so the metering side
-- stays decoupled from the money side.

create table if not exists events (
    id                  uuid primary key,
    org_id              uuid not null,
    idempotency_key     text not null,
    event_time          timestamptz not null,
    meter               text not null,
    account_id          uuid not null,
    run_id              uuid,
    properties          jsonb not null default '{}'::jsonb,
    status              text not null default 'recorded',
    supersedes_event_id uuid references events (id),
    created_at          timestamptz not null default now()
);

create unique index if not exists events_idem_idx on events (org_id, idempotency_key);
create index if not exists events_account_idx on events (account_id, event_time);
create index if not exists events_run_idx on events (run_id) where run_id is not null;
create index if not exists events_account_status_idx on events (account_id) where status = 'recorded';
