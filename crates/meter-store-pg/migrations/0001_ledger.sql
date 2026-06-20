-- Engine ledger schema: the append-only double-entry ledger and its derived account balances.
-- Money-truth lives here. Forward-only; per-account settled balances are maintained transactionally,
-- and every transfer is recorded with its counter-party (paired_account_id) for a complete audit trail.

create table if not exists ledger_accounts (
    id              uuid primary key,
    org_id          uuid not null,
    scope           text not null,
    no_overdraft    boolean not null,
    parent_id       uuid references ledger_accounts (id),
    settled_credits numeric(30, 5) not null default 0,
    created_at      timestamptz not null default now()
);

create index if not exists ledger_accounts_org_idx on ledger_accounts (org_id);
create index if not exists ledger_accounts_parent_idx on ledger_accounts (parent_id)
    where parent_id is not null;

create table if not exists ledger_entries (
    id                   uuid primary key,
    org_id               uuid not null,
    account_id           uuid not null references ledger_accounts (id),
    paired_account_id    uuid not null references ledger_accounts (id),
    entry_type           text not null,
    delta_credits        numeric(30, 5) not null,
    balance_after        numeric(30, 5) not null,
    source               text,
    revenue_recognizable boolean not null default false,
    reverses_entry_id    uuid references ledger_entries (id),
    reservation_id       uuid,
    idempotency_key      text,
    created_at           timestamptz not null default now()
);

create index if not exists ledger_entries_account_idx on ledger_entries (account_id, created_at);
create index if not exists ledger_entries_paired_idx on ledger_entries (paired_account_id);
-- grant idempotency is scoped per account
create unique index if not exists ledger_entries_idem_idx
    on ledger_entries (account_id, idempotency_key)
    where idempotency_key is not null;

create table if not exists ledger_holds (
    reservation_id  uuid primary key,
    org_id          uuid not null,
    account_id      uuid not null references ledger_accounts (id),
    amount          numeric(30, 5) not null,
    status          text not null,
    settle_entry_id uuid references ledger_entries (id),
    created_at      timestamptz not null default now()
);

create index if not exists ledger_holds_open_idx on ledger_holds (account_id)
    where status = 'open';

-- The system account (mint + usage sink). Every grant/settle pairs against it. org and id are the
-- well-known nil UUID (SYSTEM_ORG / SYSTEM_ACCOUNT). Its balance is intentionally not maintained.
insert into ledger_accounts (id, org_id, scope, no_overdraft)
values (
    '00000000-0000-0000-0000-000000000000',
    '00000000-0000-0000-0000-000000000000',
    'system',
    false
)
on conflict (id) do nothing;
