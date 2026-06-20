//! Integration test for the ClickHouse analytics store against a real ClickHouse container.

use meter_store_ch::{ChStore, EventRow};
use time::macros::datetime;
use uuid::Uuid;

use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
async fn ingests_dedups_and_aggregates_by_model() {
    let container = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let port = container.get_host_port_ipv4(8123).await.expect("http port");
    let store = ChStore::new(&format!("http://127.0.0.1:{port}"));
    store.migrate().await.expect("migrate");

    let org = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    let account = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
    let ev_a = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
    let ev_b = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();
    let ev_c = Uuid::parse_str("cccccccc-cccc-cccc-cccc-cccccccccccc").unwrap();
    // Same partition (same month) so the ReplacingMergeTree treats A v1 / A v2 as duplicates.
    let ts = datetime!(2026-06-20 12:00:00 UTC);

    let base = EventRow {
        org_id: org,
        event_id: ev_a,
        account_id: account,
        meter: "tokens".to_owned(),
        model: "claude-opus-4-8".to_owned(),
        input_tokens: 0,
        output_tokens: 0,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
        credits: 0.0,
        ts,
        version: 1,
    };

    store
        .insert_events(&[
            EventRow {
                event_id: ev_a,
                input_tokens: 100,
                output_tokens: 50,
                credits: 10.0,
                version: 1,
                ..base.clone()
            },
            // A re-ingested with a higher version (the corrected values) — must dedup to this row.
            EventRow {
                event_id: ev_a,
                input_tokens: 200,
                output_tokens: 80,
                credits: 20.0,
                version: 2,
                ..base.clone()
            },
            EventRow {
                event_id: ev_b,
                input_tokens: 300,
                output_tokens: 90,
                credits: 30.0,
                version: 1,
                ..base.clone()
            },
            EventRow {
                event_id: ev_c,
                model: "gpt-x".to_owned(),
                input_tokens: 400,
                output_tokens: 100,
                credits: 40.0,
                version: 1,
                ..base.clone()
            },
        ])
        .await
        .expect("insert events");

    // Idempotent: A appears once after dedup → three distinct events.
    let count = store.event_count(org).await.expect("count");
    assert_eq!(count, 3);

    let usage = store.usage_by_model(org).await.expect("usage by model");
    assert_eq!(usage.len(), 2);

    // Opus has the higher spend, so it sorts first; A counts once at v2 (20 credits, 200 input).
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
    assert_eq!(gpt.credits, 40.0);

    // All events share one day; credits dedup A to v2 → 20 + 30 + 40 = 90.
    let days = store.usage_by_day(org).await.expect("usage by day");
    assert_eq!(days.len(), 1);
    assert_eq!(days[0].day, "2026-06-20");
    assert_eq!(days[0].events, 3);
    assert_eq!(days[0].credits, 90.0);
}
