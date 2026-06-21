//! Tests for `meterctl reconcile` / `meterctl rebuild-rollups` against the real binary + a real
//! ClickHouse: the happy path (zero drift → success), and the ops-critical drift path (reconcile exits
//! non-zero so it can gate a cron, then rebuild-rollups repairs it and reconcile passes).

use std::process::Command;

use meter_core::{AccountId, OrgId};
use meter_event::{EventStore, RecordEvent};
use meter_store_ch::{ChStore, IngestMode};
use serde_json::json;
use time::OffsetDateTime;

use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// Run `meterctl <args>` against a ClickHouse URL and return (success, stdout).
fn meterctl(args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args(args)
        .output()
        .expect("run meterctl");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[tokio::test]
async fn reconcile_reports_consistent_rollup() {
    let container = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let port = container.get_host_port_ipv4(8123).await.expect("http port");
    let url = format!("http://127.0.0.1:{port}");
    let store = ChStore::new(&url);
    store.migrate().await.expect("migrate");

    let org = OrgId::new();
    let account = AccountId::new();
    for (i, credits) in ["10", "20"].iter().enumerate() {
        store
            .record(RecordEvent {
                org_id: org,
                idempotency_key: format!("rec-{i}"),
                event_time: OffsetDateTime::now_utc(),
                meter: "tokens".to_owned(),
                account_id: account,
                run_id: None,
                properties: json!({ "model": "gpt-5", "output": 100, "credits": credits }),
            })
            .await
            .expect("record");
    }

    let (ok, stdout) = meterctl(&[
        "reconcile",
        "--clickhouse-url",
        &url,
        "--org",
        &org.as_uuid().to_string(),
    ]);
    assert!(ok, "reconcile should succeed on consistent data");
    assert!(stdout.contains("rollups consistent"));
}

#[tokio::test]
async fn reconcile_detects_drift_then_rebuild_repairs_it() {
    let container = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let port = container.get_host_port_ipv4(8123).await.expect("http port");
    let url = format!("http://127.0.0.1:{port}");
    // Append mode skips cross-call dedup, so recording the same event twice double-counts the rollup
    // while the events SoR dedups to one — genuine drift to exercise the ops workflow.
    let store = ChStore::new(&url).with_ingest_mode(IngestMode::Append);
    store.migrate().await.expect("migrate");

    let org = OrgId::new();
    let account = AccountId::new();
    for _ in 0..2 {
        store
            .record(RecordEvent {
                org_id: org,
                idempotency_key: "dup".to_owned(),
                event_time: OffsetDateTime::now_utc(),
                meter: "tokens".to_owned(),
                account_id: account,
                run_id: None,
                properties: json!({ "model": "gpt-5", "output": 100, "credits": "10" }),
            })
            .await
            .expect("record");
    }
    let org_arg = org.as_uuid().to_string();

    // reconcile must FAIL (non-zero exit) on drift — this is what gates a cron/alert.
    let (ok, stdout) = meterctl(&["reconcile", "--clickhouse-url", &url, "--org", &org_arg]);
    assert!(!ok, "reconcile must exit non-zero on drift");
    assert!(
        stdout.contains("drifted"),
        "should report the drift: {stdout}"
    );

    // rebuild-rollups repairs it.
    let (ok, _) = meterctl(&[
        "rebuild-rollups",
        "--clickhouse-url",
        &url,
        "--org",
        &org_arg,
    ]);
    assert!(ok, "rebuild-rollups should succeed");

    // reconcile now passes.
    let (ok, stdout) = meterctl(&["reconcile", "--clickhouse-url", &url, "--org", &org_arg]);
    assert!(ok, "reconcile should pass after rebuild");
    assert!(stdout.contains("rollups consistent"));
}
