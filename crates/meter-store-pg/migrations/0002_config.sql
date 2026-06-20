-- Config the control plane syncs into the engine so it can price and enforce locally (ADR 0001:
-- the control plane authors config and computes no money; the engine holds money + the config it needs).

-- Versioned rate cards. A logical card is identified by `id`; each edit is a new immutable `version`.
-- `components` is the priced dimensional matrix as JSON. The live card for an id is its highest version.
create table if not exists rate_cards (
    id          uuid not null,
    version     integer not null,
    kind        text not null,
    currency    text not null,
    margin      numeric(20, 10) not null,
    components  jsonb not null,
    created_at  timestamptz not null default now(),
    primary key (id, version)
);

-- A spend limit applied to an account over a recurring period. One current budget per account.
create table if not exists budgets (
    account_id    uuid primary key,
    limit_credits numeric(30, 5) not null,
    period        text not null,
    updated_at    timestamptz not null default now()
);
