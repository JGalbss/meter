//! A reusable conformance suite for [`LedgerBackend`] implementations.
//!
//! Every assertion here goes through the trait only, so the same suite runs unchanged against every
//! backend — the in-memory reference and the Postgres implementation — and guarantees they behave
//! identically. Backend test modules call [`run_all_scenarios`] and drive [`check_against_model`] with
//! proptest. Enabled via the `conformance` feature (and always for this crate's own tests).

use time::OffsetDateTime;

use meter_core::{AccountId, Credit, OrgId, RunId};

use crate::backend::LedgerBackend;
use crate::error::LedgerError;
use crate::model::{AccountScope, CreditSource, LimitClass, ReservationId};
use crate::request::{
    ChargeRequest, GrantRequest, LeaseRequest, NewAccount, RefundRequest, ReserveOutcome,
    ReserveRequest, SettleRequest,
};

/// A whole-credit amount.
fn credits(n: i64) -> Credit {
    Credit::from(n)
}

async fn open_no_overdraft_org<L: LedgerBackend>(ledger: &L) -> AccountId {
    ledger
        .open_account(NewAccount {
            org_id: OrgId::new(),
            scope: AccountScope::Org,
            no_overdraft: true,
            parent_id: None,
        })
        .await
        .expect("open account")
        .id
}

/// A grant raises both settled and available balance.
pub async fn grant_increases_balance<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let balance = ledger.balance(account).await.expect("balance");
    assert_eq!(balance.settled, credits(100));
    assert_eq!(balance.available(), credits(100));
}

/// A HARD reservation beyond the available balance is denied.
pub async fn reserve_denies_when_insufficient<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(10),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let outcome = ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: Default::default(),
            amount: credits(25),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");
    assert!(matches!(outcome, ReserveOutcome::Denied { .. }));
}

/// A hold lowers available (not settled); settle charges the actual and clears the hold.
pub async fn reserve_hold_then_settle_charges_actual<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = Default::default();
    let outcome = ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");
    assert!(matches!(outcome, ReserveOutcome::Allowed { .. }));
    let held = ledger.balance(account).await.expect("balance");
    assert_eq!(held.settled, credits(100));
    assert_eq!(held.available(), credits(60));

    ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: credits(30),
        })
        .await
        .expect("settle");
    let after = ledger.balance(account).await.expect("balance");
    assert_eq!(after.settled, credits(70));
    assert_eq!(after.held, Credit::ZERO);
}

/// reserve and settle are idempotent on the reservation id.
pub async fn reserve_and_settle_are_idempotent<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = Default::default();
    let request = ReserveRequest {
        account,
        reservation_id: reservation,
        amount: credits(40),
        limit: LimitClass::Hard,
        expires_at: None,
        run_id: None,
    };
    ledger.reserve(request.clone()).await.expect("reserve");
    ledger.reserve(request).await.expect("reserve again");
    assert_eq!(
        ledger.balance(account).await.expect("balance").held,
        credits(40)
    );

    let settle = SettleRequest {
        reservation_id: reservation,
        actual: credits(30),
    };
    let first = ledger.settle(settle.clone()).await.expect("settle");
    let second = ledger.settle(settle).await.expect("settle again");
    assert_eq!(first, second);
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        credits(70)
    );
}

/// A grant is idempotent on its key, scoped to the account.
pub async fn grant_is_idempotent_on_key<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    let grant = GrantRequest {
        account,
        amount: credits(50),
        source: CreditSource::Paid,
        idempotency_key: Some("topup-1".to_owned()),
    };
    ledger.grant(grant.clone()).await.expect("grant");
    ledger.grant(grant).await.expect("grant again");
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        credits(50)
    );
}

/// Voiding a reservation returns the held credits; void is idempotent.
pub async fn void_releases_a_failed_run<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = Default::default();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");
    ledger.void(reservation).await.expect("void");
    ledger.void(reservation).await.expect("void is idempotent");
    assert_eq!(
        ledger.balance(account).await.expect("balance").available(),
        credits(100)
    );
}

/// Settling more than was held charges the full actual (the overage path): a reservation is a
/// worst-case ceiling, but billing reflects what was really used. The hold clears either way.
pub async fn settle_overage_charges_actual<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = Default::default();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");
    // The actual (60) exceeds the 40 hold; the full 60 is charged, not the held 40.
    ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: credits(60),
        })
        .await
        .expect("settle");
    let after = ledger.balance(account).await.expect("balance");
    assert_eq!(after.settled, credits(40)); // 100 − 60
    assert_eq!(after.held, Credit::ZERO);
}

/// A SOFT limit on an overdraft-allowed account reserves *beyond* the available balance (it tracks
/// overage rather than blocking), while a HARD limit on the same account denies the same request.
pub async fn soft_limit_allows_overage<L: LedgerBackend>(ledger: &L) {
    let account = ledger
        .open_account(NewAccount {
            org_id: OrgId::new(),
            scope: AccountScope::Org,
            no_overdraft: false,
            parent_id: None,
        })
        .await
        .expect("open account")
        .id;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(50),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");

    // SOFT, beyond the 50 available -> allowed; the overage is held.
    let allowed = ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: Default::default(),
            amount: credits(80),
            limit: LimitClass::Soft,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("soft reserve");
    assert!(matches!(allowed, ReserveOutcome::Allowed { .. }));
    assert_eq!(
        ledger.balance(account).await.expect("balance").held,
        credits(80)
    );

    // HARD, beyond available on the same account -> denied (a fresh reservation id).
    let denied = ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: Default::default(),
            amount: credits(80),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("hard reserve");
    assert!(matches!(denied, ReserveOutcome::Denied { .. }));
}

/// A reservation that was voided cannot then be settled — money is never charged against a released
/// hold. Settle returns [`LedgerError::ReservationClosed`] and the balance is unchanged.
pub async fn settle_after_void_is_refused<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = Default::default();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");
    ledger.void(reservation).await.expect("void");
    let settled = ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: credits(30),
        })
        .await;
    assert_eq!(settled, Err(LedgerError::ReservationClosed(reservation)));
    // The void released the hold and nothing was charged.
    let balance = ledger.balance(account).await.expect("balance");
    assert_eq!(balance.settled, credits(100));
    assert_eq!(balance.held, Credit::ZERO);
}

/// A reservation that was settled cannot then be voided — a charge is never reversed by a late void.
/// Void returns [`LedgerError::ReservationClosed`] and the settled balance stands.
pub async fn void_after_settle_is_refused<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = Default::default();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");
    ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: credits(30),
        })
        .await
        .expect("settle");
    let voided = ledger.void(reservation).await;
    assert_eq!(voided, Err(LedgerError::ReservationClosed(reservation)));
    // The settle stands: 100 − 30 charged, nothing held, nothing refunded.
    let balance = ledger.balance(account).await.expect("balance");
    assert_eq!(balance.settled, credits(70));
    assert_eq!(balance.held, Credit::ZERO);
}

/// A direct charge posts usage and lowers the balance; it is idempotent on its key.
pub async fn charge_records_usage<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let charge = ChargeRequest {
        account,
        amount: credits(30),
        idempotency_key: Some("charge-1".to_owned()),
    };
    ledger.charge(charge.clone()).await.expect("charge");
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        credits(70)
    );
    ledger
        .charge(charge)
        .await
        .expect("charge again (idempotent)");
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        credits(70)
    );
}

/// Leasing moves credits from the parent pool into a session sub-balance, conserving the total.
pub async fn lease_moves_credits_and_conserves<L: LedgerBackend>(ledger: &L) {
    let parent = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account: parent,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let lease = ledger
        .open_lease(LeaseRequest {
            parent,
            amount: credits(40),
        })
        .await
        .expect("open lease");
    assert_eq!(lease.scope, AccountScope::Session);
    assert_eq!(lease.parent_id, Some(parent));
    assert_eq!(
        ledger
            .balance(parent)
            .await
            .expect("parent balance")
            .settled,
        credits(60)
    );
    let lease_balance = ledger.balance(lease.id).await.expect("lease balance");
    assert_eq!(lease_balance.settled, credits(40));
    assert_eq!(lease_balance.available(), credits(40));
}

/// A session reserves/settles against its lease; closing returns the unused remainder to the parent.
pub async fn lease_spend_then_close_returns_remainder<L: LedgerBackend>(ledger: &L) {
    let parent = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account: parent,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let lease = ledger
        .open_lease(LeaseRequest {
            parent,
            amount: credits(40),
        })
        .await
        .expect("open lease");
    let reservation = Default::default();
    let outcome = ledger
        .reserve(ReserveRequest {
            account: lease.id,
            reservation_id: reservation,
            amount: credits(30),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve against lease");
    assert!(matches!(outcome, ReserveOutcome::Allowed { .. }));
    ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: credits(25),
        })
        .await
        .expect("settle against lease");
    assert_eq!(
        ledger
            .balance(lease.id)
            .await
            .expect("lease balance")
            .settled,
        credits(15)
    );

    let returned = ledger.close_lease(lease.id).await.expect("close lease");
    assert_eq!(returned, credits(15));
    // Parent regained the remainder: 60 (after lease) + 15 = 75; net spent over the session is 25.
    assert_eq!(
        ledger
            .balance(parent)
            .await
            .expect("parent balance")
            .settled,
        credits(75)
    );
    assert_eq!(
        ledger
            .balance(lease.id)
            .await
            .expect("lease balance")
            .settled,
        credits(0)
    );
}

/// Leasing more than a no-overdraft parent's available balance is refused, leaving the parent intact.
pub async fn over_lease_is_refused<L: LedgerBackend>(ledger: &L) {
    let parent = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account: parent,
            amount: credits(10),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let result = ledger
        .open_lease(LeaseRequest {
            parent,
            amount: credits(25),
        })
        .await;
    assert!(matches!(result, Err(LedgerError::InsufficientFunds { .. })));
    assert_eq!(
        ledger
            .balance(parent)
            .await
            .expect("parent balance")
            .settled,
        credits(10)
    );
}

/// Run every self-contained scenario against a backend (each opens its own account).
/// The sweep auto-voids open holds past their expiry, releasing the credits; unexpired and
/// non-expiring holds are untouched and still settle.
pub async fn expired_holds_are_swept<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");

    // An already-expired hold and a non-expiring hold.
    let expired = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: expired,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: Some(OffsetDateTime::UNIX_EPOCH),
            run_id: None,
        })
        .await
        .expect("reserve expired");
    let live = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: live,
            amount: credits(10),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve live");
    assert_eq!(
        ledger.balance(account).await.expect("balance").held,
        credits(50)
    );

    // Sweep: only the expired hold is released.
    let swept = ledger
        .void_expired_holds(OffsetDateTime::now_utc())
        .await
        .expect("sweep");
    assert_eq!(swept, 1);
    assert_eq!(
        ledger.balance(account).await.expect("balance").held,
        credits(10)
    );

    // The non-expiring hold still settles.
    ledger
        .settle(SettleRequest {
            reservation_id: live,
            actual: credits(10),
        })
        .await
        .expect("settle live");
    let balance = ledger.balance(account).await.expect("balance");
    assert_eq!(balance.settled, credits(90));
    assert_eq!(balance.held, Credit::ZERO);
}

/// Extending an open hold's expiry keeps it alive past a sweep; extending a closed hold is refused.
pub async fn extend_hold_keeps_it_alive<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: Some(OffsetDateTime::UNIX_EPOCH),
            run_id: None,
        })
        .await
        .expect("reserve");

    // Heartbeat: push expiry far into the future, so the sweep leaves it alone.
    let far_future = OffsetDateTime::now_utc() + time::Duration::days(3650);
    ledger
        .extend_hold(reservation, far_future)
        .await
        .expect("extend");
    assert_eq!(
        ledger
            .void_expired_holds(OffsetDateTime::now_utc())
            .await
            .expect("sweep"),
        0
    );
    assert_eq!(
        ledger.balance(account).await.expect("balance").held,
        credits(40)
    );

    // Once settled, the hold cannot be extended.
    ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: credits(30),
        })
        .await
        .expect("settle");
    assert_eq!(
        ledger.extend_hold(reservation, far_future).await,
        Err(LedgerError::ReservationClosed(reservation))
    );
}

/// A refund (credit-note) adds credits back, references the entry it reverses, and is idempotent.
pub async fn refund_credits_back<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");
    let settle = ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: credits(30),
        })
        .await
        .expect("settle");
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        credits(70)
    );

    // Credit the 30 back, referencing the settle entry.
    let refund = ledger
        .refund(RefundRequest {
            account,
            amount: credits(30),
            reverses_entry_id: Some(settle.id),
            idempotency_key: Some("refund-1".to_owned()),
        })
        .await
        .expect("refund");
    assert_eq!(refund.reverses_entry_id, Some(settle.id));
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        credits(100)
    );

    // Idempotent on the key.
    let again = ledger
        .refund(RefundRequest {
            account,
            amount: credits(30),
            reverses_entry_id: Some(settle.id),
            idempotency_key: Some("refund-1".to_owned()),
        })
        .await
        .expect("refund again");
    assert_eq!(again.id, refund.id);
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        credits(100)
    );

    // A non-positive refund is refused.
    assert_eq!(
        ledger
            .refund(RefundRequest {
                account,
                amount: credits(0),
                reverses_entry_id: None,
                idempotency_key: None,
            })
            .await,
        Err(LedgerError::NonPositiveAmount)
    );
}

/// Voiding a run reverses exactly that run's financial impact: its open holds are released and its
/// settled charges refunded, while holds from other runs are untouched. The reversal is idempotent.
pub async fn void_run_reverses_holds_and_settles<L: LedgerBackend>(ledger: &L) {
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(100),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");

    let run = RunId::new();
    // An open hold in the run.
    let open_hold = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: open_hold,
            amount: credits(40),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: Some(run),
        })
        .await
        .expect("reserve open");
    // A settled charge in the same run.
    let settled_hold = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: settled_hold,
            amount: credits(30),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: Some(run),
        })
        .await
        .expect("reserve settled");
    ledger
        .settle(SettleRequest {
            reservation_id: settled_hold,
            actual: credits(20),
        })
        .await
        .expect("settle");
    // A hold belonging to a *different* run — must survive the void untouched.
    let other_run = RunId::new();
    let other_hold = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: other_hold,
            amount: credits(10),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: Some(other_run),
        })
        .await
        .expect("reserve other");

    // Before voiding: settled 80 (100 − 20), held 50 (40 open + 10 other).
    let before = ledger.balance(account).await.expect("balance");
    assert_eq!(before.settled, credits(80));
    assert_eq!(before.held, credits(50));

    let summary = ledger.void_run(run).await.expect("void run");
    assert_eq!(summary.holds_released, 1);
    assert_eq!(summary.charges_refunded, 1);
    assert_eq!(summary.credits_refunded, credits(20));

    // After: the settle is refunded (settled back to 100) and the open hold released; only the
    // other run's 10 remains held.
    let after = ledger.balance(account).await.expect("balance");
    assert_eq!(after.settled, credits(100));
    assert_eq!(after.held, credits(10));

    // The released hold is now closed — settling it is refused.
    assert_eq!(
        ledger
            .settle(SettleRequest {
                reservation_id: open_hold,
                actual: credits(5),
            })
            .await,
        Err(LedgerError::ReservationClosed(open_hold))
    );

    // Idempotent: a second void releases nothing new and never double-refunds.
    let again = ledger.void_run(run).await.expect("void run again");
    assert_eq!(again.holds_released, 0);
    assert_eq!(again.charges_refunded, 0);
    assert_eq!(again.credits_refunded, credits(0));
    let stable = ledger.balance(account).await.expect("balance");
    assert_eq!(stable.settled, credits(100));
    assert_eq!(stable.held, credits(10));

    // The other run's hold is still open and settleable.
    ledger
        .settle(SettleRequest {
            reservation_id: other_hold,
            actual: credits(10),
        })
        .await
        .expect("other settle");
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        credits(90)
    );
}

pub async fn run_all_scenarios<L: LedgerBackend>(ledger: &L) {
    grant_increases_balance(ledger).await;
    reserve_denies_when_insufficient(ledger).await;
    refund_credits_back(ledger).await;
    void_run_reverses_holds_and_settles(ledger).await;
    expired_holds_are_swept(ledger).await;
    extend_hold_keeps_it_alive(ledger).await;
    reserve_hold_then_settle_charges_actual(ledger).await;
    reserve_and_settle_are_idempotent(ledger).await;
    grant_is_idempotent_on_key(ledger).await;
    void_releases_a_failed_run(ledger).await;
    settle_overage_charges_actual(ledger).await;
    soft_limit_allows_overage(ledger).await;
    settle_after_void_is_refused(ledger).await;
    void_after_settle_is_refused(ledger).await;
    charge_records_usage(ledger).await;
    lease_moves_credits_and_conserves(ledger).await;
    lease_spend_then_close_returns_remainder(ledger).await;
    over_lease_is_refused(ledger).await;
}

/// One hold in a [`void_run_property`] spec: a reservation tagged with a run, either left open or
/// settled at an actual amount.
#[derive(Debug, Clone)]
pub struct HoldSpec {
    /// Which run this hold belongs to (a small index; mapped to a fresh `RunId`).
    pub run: u8,
    /// Reserve amount (clamped to at least 1).
    pub amount: u32,
    /// `None` leaves the hold open; `Some(actual)` settles it at `min(actual, amount)`.
    pub settle: Option<u32>,
}

/// Property: voiding a run reverses exactly that run's financial impact and nothing else, and is
/// idempotent — over an arbitrary set of run-tagged holds. Releases the target run's open holds and
/// refunds its non-zero settled charges; other runs are untouched; credits are conserved. Drive this
/// with proptest from each backend's tests, the same way as [`check_against_model`].
pub async fn void_run_property<L: LedgerBackend>(ledger: &L, specs: &[HoldSpec], target_run: u8) {
    // Fund strictly above the sum of all reserve amounts so every HARD reserve is allowed.
    let total: i64 = specs.iter().map(|spec| i64::from(spec.amount.max(1))).sum();
    let account = open_no_overdraft_org(ledger).await;
    ledger
        .grant(GrantRequest {
            account,
            amount: credits(total + 1),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");

    // A fresh RunId per run index in play (specs and the target).
    let run_count = specs
        .iter()
        .map(|spec| spec.run)
        .chain(std::iter::once(target_run))
        .max()
        .map_or(1, |max| usize::from(max) + 1);
    let run_ids: Vec<RunId> = (0..run_count).map(|_| RunId::new()).collect();

    // Apply every hold, accumulating the expected effect of voiding the target run.
    let mut target_open_amount: i64 = 0;
    let mut target_open_count: u64 = 0;
    let mut target_refund_amount: i64 = 0;
    let mut target_refund_count: u64 = 0;
    for spec in specs {
        let amount = i64::from(spec.amount.max(1));
        let reservation = ReservationId::new();
        let outcome = ledger
            .reserve(ReserveRequest {
                account,
                reservation_id: reservation,
                amount: credits(amount),
                limit: LimitClass::Hard,
                expires_at: None,
                run_id: Some(run_ids[usize::from(spec.run)]),
            })
            .await
            .expect("reserve");
        assert!(
            matches!(outcome, ReserveOutcome::Allowed { .. }),
            "over-funded reserve was denied"
        );
        let is_target = spec.run == target_run;
        match spec.settle {
            None => {
                if is_target {
                    target_open_amount += amount;
                    target_open_count += 1;
                }
            }
            Some(actual) => {
                let actual = i64::from(actual).min(amount);
                ledger
                    .settle(SettleRequest {
                        reservation_id: reservation,
                        actual: credits(actual),
                    })
                    .await
                    .expect("settle");
                // Only a positive settled charge is refundable on void.
                if is_target && actual > 0 {
                    target_refund_amount += actual;
                    target_refund_count += 1;
                }
            }
        }
    }

    let before = ledger.balance(account).await.expect("balance before");
    let summary = ledger
        .void_run(run_ids[usize::from(target_run)])
        .await
        .expect("void run");
    assert_eq!(summary.holds_released, target_open_count, "holds_released");
    assert_eq!(
        summary.charges_refunded, target_refund_count,
        "charges_refunded"
    );
    assert_eq!(
        summary.credits_refunded,
        credits(target_refund_amount),
        "credits_refunded"
    );

    // Settled rises by the refunded charges; held falls by the released holds; other runs untouched.
    let after = ledger.balance(account).await.expect("balance after");
    assert_eq!(
        after.settled,
        before.settled + credits(target_refund_amount),
        "settled after void"
    );
    assert_eq!(
        after.held,
        before.held - credits(target_open_amount),
        "held after void"
    );

    // Idempotent: a second void reverses nothing new and leaves the balance unchanged.
    let again = ledger
        .void_run(run_ids[usize::from(target_run)])
        .await
        .expect("void run again");
    assert_eq!(again.holds_released, 0, "idempotent holds_released");
    assert_eq!(again.charges_refunded, 0, "idempotent charges_refunded");
    assert_eq!(
        again.credits_refunded,
        credits(0),
        "idempotent credits_refunded"
    );
    let stable = ledger.balance(account).await.expect("balance stable");
    assert_eq!(
        stable.settled, after.settled,
        "settled stable after re-void"
    );
    assert_eq!(stable.held, after.held, "held stable after re-void");
}

/// One operation in a model-based conformance sequence.
#[derive(Debug, Clone)]
pub enum Op {
    Grant(u32),
    Spend {
        reserve: u32,
        actual: u32,
    },
    /// Reserve then void: a released hold must never change the settled balance.
    Void {
        reserve: u32,
    },
}

/// Apply a sequence of ops to a fresh no-overdraft account, asserting after each step that the
/// backend's balance matches an independently-computed model and never overdrafts. This is the core
/// cross-backend invariant; drive it with proptest from each backend's tests.
pub async fn check_against_model<L: LedgerBackend>(ledger: &L, ops: &[Op]) {
    let account = open_no_overdraft_org(ledger).await;
    let mut expected_settled: i64 = 0;
    for op in ops {
        match *op {
            Op::Grant(amount) => {
                let amount = i64::from(amount);
                ledger
                    .grant(GrantRequest {
                        account,
                        amount: credits(amount),
                        source: CreditSource::Paid,
                        idempotency_key: None,
                    })
                    .await
                    .expect("grant");
                expected_settled += amount;
            }
            Op::Spend { reserve, actual } => {
                let actual = actual.min(reserve);
                let reserve = i64::from(reserve);
                let actual = i64::from(actual);
                let reservation = Default::default();
                let outcome = ledger
                    .reserve(ReserveRequest {
                        account,
                        reservation_id: reservation,
                        amount: credits(reserve),
                        limit: LimitClass::Hard,
                        expires_at: None,
                        run_id: None,
                    })
                    .await
                    .expect("reserve");
                if expected_settled >= reserve {
                    assert!(
                        matches!(outcome, ReserveOutcome::Allowed { .. }),
                        "expected allow with {expected_settled} available for reserve {reserve}"
                    );
                    ledger
                        .settle(SettleRequest {
                            reservation_id: reservation,
                            actual: credits(actual),
                        })
                        .await
                        .expect("settle");
                    expected_settled -= actual;
                } else {
                    assert!(
                        matches!(outcome, ReserveOutcome::Denied { .. }),
                        "expected deny with {expected_settled} available for reserve {reserve}"
                    );
                }
            }
            Op::Void { reserve } => {
                let reserve = i64::from(reserve);
                let reservation = Default::default();
                ledger
                    .reserve(ReserveRequest {
                        account,
                        reservation_id: reservation,
                        amount: credits(reserve),
                        limit: LimitClass::Hard,
                        expires_at: None,
                        run_id: None,
                    })
                    .await
                    .expect("reserve");
                // Voiding releases the hold (or no-ops a denied reservation): settled never moves.
                ledger.void(reservation).await.expect("void");
            }
        }
        let balance = ledger.balance(account).await.expect("balance");
        assert_eq!(
            balance.settled,
            credits(expected_settled),
            "settled mismatch"
        );
        assert_eq!(
            balance.held,
            Credit::ZERO,
            "held should be zero between ops"
        );
        assert!(!balance.available().is_negative(), "overdraft");
    }
}
