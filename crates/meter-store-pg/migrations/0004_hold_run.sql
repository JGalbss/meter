-- Tag holds with the agent run they belong to, so `void_run` can reverse a whole run's financial
-- impact in one pass: release its open holds and refund its settled charges. NULL = not part of a run.
-- The partial index keeps the run lookup cheap — it only covers run-tagged holds.
alter table ledger_holds add column if not exists run_id uuid;

create index if not exists ledger_holds_run_idx on ledger_holds (run_id)
    where run_id is not null;
