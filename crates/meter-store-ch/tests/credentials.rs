//! Integration test: the `ClickHouse` store authenticates with non-default credentials against a named
//! database. `ClickHouse` rejects a passwordless user over a remote connection, so a networked engine
//! must supply credentials — this proves `with_credentials`/`with_database` wire through to a real
//! server (the gap that crash-looped the cross-stack e2e; see `deploy/e2e/README.md`).

use meter_core::{AccountId, OrgId, RunId};
use meter_event::{EventStore, RecordEvent};
use meter_store_ch::ChStore;
use serde_json::json;
use time::macros::datetime;
use uuid::Uuid;

use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;

#[tokio::test]
async fn authenticates_with_credentials_and_named_database() {
    // A ClickHouse server that requires a password and serves a non-default database.
    let container = ClickHouse::default()
        .with_env_var("CLICKHOUSE_USER", "meter")
        .with_env_var("CLICKHOUSE_PASSWORD", "s3cret")
        .with_env_var("CLICKHOUSE_DB", "metering")
        .start()
        .await
        .expect("start clickhouse");
    let port = container.get_host_port_ipv4(8123).await.expect("http port");
    let url = format!("http://127.0.0.1:{port}");

    // Authenticated client against the named database: migrate + record + read all succeed.
    let store = ChStore::new(&url)
        .with_credentials("meter", "s3cret")
        .with_database("metering");
    store.migrate().await.expect("authenticated migrate");

    let org = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    let account = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
    let run = Uuid::parse_str("a0000000-0000-0000-0000-000000000000").unwrap();
    store
        .record(RecordEvent {
            org_id: OrgId::from_uuid(org),
            idempotency_key: "k1".to_owned(),
            event_time: datetime!(2026-06-20 12:00:00 UTC),
            meter: "tokens".to_owned(),
            account_id: AccountId::from_uuid(account),
            run_id: Some(RunId::from_uuid(run)),
            properties: json!({
                "model": "claude-opus-4-8",
                "input_uncached": 200,
                "cache_read": 0,
                "cache_write": 0,
                "output": 80,
                "reasoning": 0,
                "credits": "20",
            }),
        })
        .await
        .expect("authenticated record");
    assert_eq!(store.event_count(org).await.expect("count"), 1);

    // A wrong password is rejected — proving the credentials are actually enforced, not ignored.
    let wrong_password = ChStore::new(&url)
        .with_credentials("meter", "wrong")
        .with_database("metering");
    assert!(
        wrong_password.event_count(org).await.is_err(),
        "a wrong password must be rejected by the server"
    );
}
