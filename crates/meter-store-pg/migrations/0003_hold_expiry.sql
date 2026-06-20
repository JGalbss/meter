-- Hold expiry (auto-void of stranded reservations). An open hold past `expires_at` is released by the
-- sweep (`void_expired_holds`); NULL never expires. The partial index makes the sweep cheap — it only
-- scans open holds.
alter table ledger_holds add column if not exists expires_at timestamptz;

create index if not exists ledger_holds_expiry_idx on ledger_holds (expires_at)
    where status = 'open';
