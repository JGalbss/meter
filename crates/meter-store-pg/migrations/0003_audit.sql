-- Immutable, append-only audit log of every mutating action against the engine, for admin review.

create table if not exists audit_log (
    id         uuid primary key,
    actor      text not null,
    method     text not null,
    path       text not null,
    status     int not null,
    created_at timestamptz not null default now()
);

create index if not exists audit_log_created_idx on audit_log (created_at desc, id desc);
create index if not exists audit_log_actor_idx on audit_log (actor);
