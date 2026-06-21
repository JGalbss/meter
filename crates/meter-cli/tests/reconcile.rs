//! Test for `meterctl reconcile`: it reports the pre-aggregated usage rollup as consistent with the
//! event store of record. Seeds events via the ClickHouse store, then runs the real binary against the
//! same ClickHouse and asserts the happy path (zero drift → success).

use std::process::Command;

use meter_core::{AccountId, OrgId};
use meter_event::{EventStore, RecordEvent};
use meter_store_ch::ChStore;
use serde_json::json;
use time::OffsetDateTime;

use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

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

    let output = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args([
            "reconcile",
            "--clickhouse-url",
            &url,
            "--org",
            &org.as_uuid().to_string(),
        ])
        .output()
        .expect("run meterctl reconcile");
    assert!(
        output.status.success(),
        "reconcile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("rollup consistent"));
}
