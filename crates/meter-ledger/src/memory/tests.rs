//! Unit and property tests for the in-memory ledger — the conformance suite the database backends
//! will be run against verbatim.

use proptest::prelude::*;
use rust_decimal::Decimal;

use crate::backend::LedgerBackend;
use crate::model::{AccountScope, CreditSource, LimitClass};
use crate::request::{GrantRequest, NewAccount, ReserveOutcome, ReserveRequest, SettleRequest};

use super::InMemoryLedger;
use meter_core::{AccountId, Credit};

fn credits(n: u32) -> Credit {
    Credit::from_decimal(Decimal::from(n))
}

fn current_thread_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build runtime")
}

async fn open_org(ledger: &InMemoryLedger) -> AccountId {
    ledger
        .open_account(NewAccount {
            scope: AccountScope::Org,
            no_overdraft: true,
            parent_id: None,
        })
        .await
        .expect("open account")
        .id
}

#[tokio::test]
async fn grant_increases_balance_and_conserves() {
    let ledger = InMemoryLedger::new();
    let account = open_org(&ledger).await;
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
    assert!(ledger.net_settled().is_zero());
}

#[tokio::test]
async fn reserve_denies_when_insufficient() {
    let ledger = InMemoryLedger::new();
    let account = open_org(&ledger).await;
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

#[tokio::test]
async fn reserve_holds_then_settle_charges_actual() {
    let ledger = InMemoryLedger::new();
    let account = open_org(&ledger).await;
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
    // While the hold is open, available drops but settled does not.
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
    assert!(ledger.net_settled().is_zero());
}

#[tokio::test]
async fn reserve_and_settle_are_idempotent() {
    let ledger = InMemoryLedger::new();
    let account = open_org(&ledger).await;
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

#[tokio::test]
async fn grant_is_idempotent_on_key() {
    let ledger = InMemoryLedger::new();
    let account = open_org(&ledger).await;
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

#[tokio::test]
async fn void_releases_a_failed_run() {
    let ledger = InMemoryLedger::new();
    let account = open_org(&ledger).await;
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

#[derive(Debug, Clone)]
enum Op {
    Grant(u32),
    Spend { reserve: u32, actual: u32 },
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1u32..1000).prop_map(Op::Grant),
        (1u32..500, 0u32..500).prop_map(|(reserve, actual)| Op::Spend {
            reserve,
            actual: actual.min(reserve),
        }),
    ]
}

proptest! {
    /// Across any sequence of grants and gated spends: the ledger always conserves (sum of all
    /// balances is zero) and a no-overdraft account never goes negative.
    #[test]
    fn conserves_and_never_overdrafts(ops in prop::collection::vec(op_strategy(), 0..300)) {
        let runtime = current_thread_runtime();
        runtime.block_on(async {
            let ledger = InMemoryLedger::new();
            let account = open_org(&ledger).await;
            for op in ops {
                match op {
                    Op::Grant(amount) => {
                        ledger
                            .grant(GrantRequest {
                                account,
                                amount: credits(amount),
                                source: CreditSource::Paid,
                                idempotency_key: None,
                            })
                            .await
                            .expect("grant");
                    }
                    Op::Spend { reserve, actual } => {
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
                        if matches!(outcome, ReserveOutcome::Allowed { .. }) {
                            ledger
                                .settle(SettleRequest {
                                    reservation_id: reservation,
                                    actual: credits(actual),
                                })
                                .await
                                .expect("settle");
                        }
                    }
                }
                assert!(ledger.net_settled().is_zero(), "conservation violated");
                let balance = ledger.balance(account).await.expect("balance");
                assert!(!balance.available().is_negative(), "overdraft");
            }
        });
    }
}
