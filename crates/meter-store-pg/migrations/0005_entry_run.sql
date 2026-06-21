-- Tag ledger entries with their agent run so a failed run's direct charges (post-hoc /v1/usage
-- metering, which posts a usage entry with no reservation) can be reversed by void_run — not just its
-- reservation holds/settles. Additive and nullable: entries written before this migration keep run_id
-- NULL and remain reversible only via manual credit-notes (documented, no silent behaviour change).
alter table ledger_entries add column if not exists run_id uuid;

create index if not exists ledger_entries_run_idx on ledger_entries (run_id)
    where run_id is not null;
