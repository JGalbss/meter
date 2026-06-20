//! A reusable conformance suite for [`LedgerBackend`] implementations.
//!
//! Every assertion here goes through the trait only, so the same suite runs unchanged against every
//! backend — the in-memory reference and the Postgres implementation — and guarantees they behave
//! identically. Backend test modules call [`run_all_scenarios`] and drive [`check_against_model`] with
//! proptest. Enabled via the `conformance` feature (and always for this crate's own tests).

use meter_core::{AccountId, Credit, OrgId};

use crate::backend::LedgerBackend;
use crate::error::LedgerError;
use crate::model::{AccountScope, CreditSource, LimitClass};
use crate::request::{
    ChargeRequest, GrantRequest, LeaseRequest, NewAccount, ReserveOutcome, ReserveRequest,
    SettleRequest,
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
pub async fn run_all_scenarios<L: LedgerBackend>(ledger: &L) {
    grant_increases_balance(ledger).await;
    reserve_denies_when_insufficient(ledger).await;
    reserve_hold_then_settle_charges_actual(ledger).await;
    reserve_and_settle_are_idempotent(ledger).await;
    grant_is_idempotent_on_key(ledger).await;
    void_releases_a_failed_run(ledger).await;
    settle_after_void_is_refused(ledger).await;
    void_after_settle_is_refused(ledger).await;
    charge_records_usage(ledger).await;
    lease_moves_credits_and_conserves(ledger).await;
    lease_spend_then_close_returns_remainder(ledger).await;
    over_lease_is_refused(ledger).await;
}

/// One operation in a model-based conformance sequence.
#[derive(Debug, Clone)]
pub enum Op {
    Grant(u32),
    Spend { reserve: u32, actual: u32 },
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
