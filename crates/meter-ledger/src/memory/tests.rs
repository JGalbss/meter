//! Tests for the in-memory ledger. The behavioural assertions live in the shared `conformance` suite
//! (so the Postgres backend runs the identical checks); here we wire that suite to `InMemoryLedger` and
//! add the in-memory-only conservation invariant (sum of all balances is exactly zero).

use proptest::prelude::*;

use super::InMemoryLedger;
use crate::conformance::{self, HoldSpec, Op};

#[tokio::test]
async fn passes_the_conformance_scenarios() {
    let ledger = InMemoryLedger::new();
    conformance::run_all_scenarios(&ledger).await;
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1u32..1000).prop_map(Op::Grant),
        (1u32..500, 0u32..500).prop_map(|(reserve, actual)| Op::Spend { reserve, actual }),
        (1u32..500).prop_map(|reserve| Op::Void { reserve }),
    ]
}

fn current_thread_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build runtime")
}

fn hold_spec_strategy() -> impl Strategy<Value = HoldSpec> {
    // Up to four runs; settle is absent (open) or an actual at/under the reserve.
    (0u8..4, 1u32..1000, prop::option::of(0u32..1000)).prop_map(|(run, amount, settle)| HoldSpec {
        run,
        amount,
        settle,
    })
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

    /// Voiding a run reverses exactly that run's holds/settles and nothing else, idempotently, over an
    /// arbitrary mix of run-tagged holds — and the in-memory ledger still conserves afterwards.
    #[test]
    fn void_run_reverses_only_its_own_and_conserves(
        specs in prop::collection::vec(hold_spec_strategy(), 0..40),
        target in 0u8..4,
    ) {
        let runtime = current_thread_runtime();
        runtime.block_on(async {
            let ledger = InMemoryLedger::new();
            conformance::void_run_property(&ledger, &specs, target).await;
            assert!(ledger.net_settled().is_zero(), "conservation violated after void_run");
        });
    }
}
