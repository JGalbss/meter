//! Integration test for the `ClickHouse` event store + usage analytics against a real `ClickHouse`
//! container. Analytics are derived from the `events` system of record, so this drives the real
//! ingest path (`record` / `amend` / `void_run`) and asserts that idempotency, amends, and voids are
//! reflected correctly in the aggregates.

use meter_core::{AccountId, EventId, OrgId, RunId};
use meter_event::{AmendEvent, EventStore, RecordEvent};
use meter_store_ch::{ChStore, DeadLetter};
use serde_json::{json, Value};
use time::macros::datetime;
use uuid::Uuid;

use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// A usage event's JSON `properties` in the shape the metering path records (credits as a string).
fn usage_props(model: &str, input_uncached: u64, output: u64, credits: &str) -> Value {
    json!({
        "model": model,
        "input_uncached": input_uncached,
        "cache_read": 0,
        "cache_write": 0,
        "output": output,
        "reasoning": 0,
        "credits": credits,
    })
}

async fn record(
    store: &ChStore,
    account: Uuid,
    key: &str,
    run: Uuid,
    properties: Value,
) -> EventId {
    store
        .record(RecordEvent {
            org_id: OrgId::from_uuid(
                Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            ),
            idempotency_key: key.to_owned(),
            event_time: datetime!(2026-06-20 12:00:00 UTC),
            meter: "tokens".to_owned(),
            account_id: AccountId::from_uuid(account),
            run_id: Some(RunId::from_uuid(run)),
            properties,
        })
        .await
        .expect("record")
        .id
}

#[tokio::test]
async fn aggregates_reflect_idempotency_amends_and_voids() {
    let container = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let port = container.get_host_port_ipv4(8123).await.expect("http port");
    let store = ChStore::new(&format!("http://127.0.0.1:{port}"));
    store.migrate().await.expect("migrate");

    let org = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    let account = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
    let run_a = Uuid::parse_str("a0000000-0000-0000-0000-000000000000").unwrap();
    let run_b = Uuid::parse_str("b0000000-0000-0000-0000-000000000000").unwrap();
    let run_c = Uuid::parse_str("c0000000-0000-0000-0000-000000000000").unwrap();
    let run_d = Uuid::parse_str("d0000000-0000-0000-0000-000000000000").unwrap();

    // A (opus): 200 input / 80 output / 20 credits. Recorded twice with the same key — idempotent.
    let a = record(
        &store,
        account,
        "a",
        run_a,
        usage_props("claude-opus-4-8", 200, 80, "20"),
    )
    .await;
    let a_again = record(
        &store,
        account,
        "a",
        run_a,
        usage_props("claude-opus-4-8", 200, 80, "20"),
    )
    .await;
    assert_eq!(
        a_again, a,
        "same idempotency key must not create a second event"
    );

    // Amend A to a corrected version with the same usage totals plus a note — must count once, at the
    // corrected version (the original becomes `amended` and drops out of the aggregates).
    let mut corrected = usage_props("claude-opus-4-8", 200, 80, "20");
    corrected["note"] = json!("corrected");
    store
        .amend(AmendEvent {
            event_id: a,
            properties: corrected,
        })
        .await
        .expect("amend");

    // B (opus): 300 / 90 / 30. C (gpt-x): 400 / 100 / 40.
    record(
        &store,
        account,
        "b",
        run_b,
        usage_props("claude-opus-4-8", 300, 90, "30"),
    )
    .await;
    record(
        &store,
        account,
        "c",
        run_c,
        usage_props("gpt-x", 400, 100, "40"),
    )
    .await;

    // D (opus) is recorded then its run is voided — it must vanish from every aggregate.
    record(
        &store,
        account,
        "d",
        run_d,
        usage_props("claude-opus-4-8", 999, 999, "999"),
    )
    .await;
    let voided = store.void_run(RunId::from_uuid(run_d)).await.expect("void");
    assert_eq!(voided, 1);

    // Three live events remain: A (corrected), B, C.
    assert_eq!(store.event_count(org).await.expect("count"), 3);

    // Usage by model: opus = A + B (events 2, input 500, output 170, credits 50); gpt-x = C.
    let usage = store.usage_by_model(org).await.expect("usage by model");
    assert_eq!(usage.len(), 2);
    let opus = &usage[0];
    assert_eq!(opus.model, "claude-opus-4-8");
    assert_eq!(opus.events, 2);
    assert_eq!(opus.input_tokens, 500);
    assert_eq!(opus.output_tokens, 170);
    assert_eq!(opus.credits, 50.0);
    let gpt = &usage[1];
    assert_eq!(gpt.model, "gpt-x");
    assert_eq!(gpt.events, 1);
    assert_eq!(gpt.input_tokens, 400);
    assert_eq!(gpt.output_tokens, 100);
    assert_eq!(gpt.credits, 40.0);

    // All on one day: 3 events, 20 + 30 + 40 = 90 credits (the voided D excluded).
    let days = store.usage_by_day(org).await.expect("usage by day");
    assert_eq!(days.len(), 1);
    assert_eq!(days[0].day, "2026-06-20");
    assert_eq!(days[0].events, 3);
    assert_eq!(days[0].credits, 90.0);

    // Dead-letter: a malformed event is captured for inspection/replay.
    assert_eq!(store.dead_letter_count(org).await.expect("dl count"), 0);
    store
        .record_dead_letter(&[DeadLetter {
            id: Uuid::parse_str("dddddddd-dddd-dddd-dddd-dddddddddddd").unwrap(),
            org_id: org,
            source: "ingest".to_owned(),
            payload: r#"{"meter":"tokens","bad":true}"#.to_owned(),
            error: "missing account".to_owned(),
            received_at: datetime!(2026-06-20 12:00:00 UTC),
        }])
        .await
        .expect("record dead letter");
    assert_eq!(store.dead_letter_count(org).await.expect("dl count"), 1);
    let dead = store.list_dead_letter(org).await.expect("list dead letter");
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].source, "ingest");
    assert_eq!(dead[0].error, "missing account");
}
