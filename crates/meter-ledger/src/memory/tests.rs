//! Tests for the in-memory ledger. The behavioural assertions live in the shared `conformance` suite
//! (so the Postgres backend runs the identical checks); here we wire that suite to `InMemoryLedger` and
//! add the in-memory-only conservation invariant (sum of all balances is exactly zero).

use proptest::prelude::*;

use super::InMemoryLedger;
use crate::conformance::{self, Op};

#[tokio::test]
async fn passes_the_conformance_scenarios() {
    let ledger = InMemoryLedger::new();
    conformance::run_all_scenarios(&ledger).await;
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1u32..1000).prop_map(Op::Grant),
        (1u32..500, 0u32..500).prop_map(|(reserve, actual)| Op::Spend { reserve, actual }),
    ]
}

fn current_thread_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build runtime")
}

proptest! {
    /// Across any sequence of grants and gated spends the backend matches the model and never
    /// overdrafts, and the in-memory ledger additionally conserves (sum of all balances is zero).
    #[test]
    fn conserves_and_never_overdrafts(ops in prop::collection::vec(op_strategy(), 0..300)) {
        let runtime = current_thread_runtime();
        runtime.block_on(async {
            let ledger = InMemoryLedger::new();
            conformance::check_against_model(&ledger, &ops).await;
            assert!(ledger.net_settled().is_zero(), "conservation violated");
        });
    }
}
