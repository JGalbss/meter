//! Reusable conformance suite for [`EventStore`] implementations. Every backend runs it unchanged.

use meter_core::{AccountId, OrgId, RunId};
use serde_json::json;
use time::OffsetDateTime;

use crate::event::EventStatus;
use crate::request::{AmendEvent, RecordEvent};
use crate::store::EventStore;

fn record_req(org: OrgId, account: AccountId, key: &str, run: Option<RunId>) -> RecordEvent {
    RecordEvent {
        org_id: org,
        idempotency_key: key.to_owned(),
        event_time: OffsetDateTime::UNIX_EPOCH,
        meter: "tokens".to_owned(),
        account_id: account,
        run_id: run,
        properties: json!({ "input": 1000, "output": 500, "model": "claude-opus-4-8" }),
    }
}

/// A recorded event can be fetched and carries its custom fields.
pub async fn record_and_get<S: EventStore>(store: &S) {
    let (org, account) = (OrgId::new(), AccountId::new());
    let event = store
        .record(record_req(org, account, "evt-1", None))
        .await
        .expect("record");
    let fetched = store.get(event.id).await.expect("get");
    assert_eq!(fetched.id, event.id);
    assert_eq!(fetched.properties["model"], json!("claude-opus-4-8"));
}

/// Recording with the same key is idempotent.
pub async fn record_is_idempotent<S: EventStore>(store: &S) {
    let (org, account) = (OrgId::new(), AccountId::new());
    let first = store
        .record(record_req(org, account, "evt-dup", None))
        .await
        .expect("record");
    let second = store
        .record(record_req(org, account, "evt-dup", None))
        .await
        .expect("record again");
    assert_eq!(first.id, second.id);
    assert_eq!(
        store.list_for_account(account).await.expect("list").len(),
        1
    );
}

/// Amending records a new version; the original becomes `Amended` and reads return the latest.
pub async fn amend_supersedes<S: EventStore>(store: &S) {
    let (org, account) = (OrgId::new(), AccountId::new());
    let original = store
        .record(record_req(org, account, "evt-amend", None))
        .await
        .expect("record");
    let amended = store
        .amend(AmendEvent {
            event_id: original.id,
            properties: json!({ "input": 1200, "output": 600 }),
        })
        .await
        .expect("amend");
    assert_eq!(amended.supersedes, Some(original.id));
    assert_eq!(
        store.get(original.id).await.expect("get").status,
        EventStatus::Amended
    );
    let current = store.list_for_account(account).await.expect("list");
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].id, amended.id);
    assert_eq!(current[0].properties["input"], json!(1200));
}

/// A batch records every event, returns them in request order, and is idempotent per key.
pub async fn record_batch_is_idempotent_and_ordered<S: EventStore>(store: &S) {
    let (org, account) = (OrgId::new(), AccountId::new());
    let reqs = vec![
        record_req(org, account, "batch-1", None),
        record_req(org, account, "batch-2", None),
        record_req(org, account, "batch-3", None),
    ];
    let recorded = store.record_batch(reqs).await.expect("record_batch");
    assert_eq!(recorded.len(), 3);
    assert_eq!(recorded[0].idempotency_key, "batch-1");
    assert_eq!(recorded[2].idempotency_key, "batch-3");

    // Re-recording overlapping keys (mixed batch + single) is idempotent: ids are stable and the
    // live set never double-counts a key.
    let again = store
        .record_batch(vec![
            record_req(org, account, "batch-2", None),
            record_req(org, account, "batch-4", None),
        ])
        .await
        .expect("record_batch again");
    assert_eq!(again[0].id, recorded[1].id);
    let single = store
        .record(record_req(org, account, "batch-1", None))
        .await
        .expect("record");
    assert_eq!(single.id, recorded[0].id);
    assert_eq!(
        store.list_for_account(account).await.expect("list").len(),
        4
    );
}

/// Voiding a run voids every current event of that run.
pub async fn void_run_voids_events<S: EventStore>(store: &S) {
    let (org, account, run) = (OrgId::new(), AccountId::new(), RunId::new());
    store
        .record(record_req(org, account, "run-evt-1", Some(run)))
        .await
        .expect("record");
    store
        .record(record_req(org, account, "run-evt-2", Some(run)))
        .await
        .expect("record");
    assert_eq!(store.void_run(run).await.expect("void_run"), 2);
    assert!(store
        .list_for_account(account)
        .await
        .expect("list")
        .is_empty());
}

/// Run every scenario against a backend.
pub async fn run_all_scenarios<S: EventStore>(store: &S) {
    record_and_get(store).await;
    record_is_idempotent(store).await;
    record_batch_is_idempotent_and_ordered(store).await;
    amend_supersedes(store).await;
    void_run_voids_events(store).await;
}
