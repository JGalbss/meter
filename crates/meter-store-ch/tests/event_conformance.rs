//! The ClickHouse `EventStore` backend must pass the identical shared event conformance suite as the
//! in-memory reference, executed against a real ClickHouse started by testcontainers (ADR 0003).

use meter_event::conformance;
use meter_store_ch::ChStore;

use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
async fn clickhouse_event_store_passes_the_shared_conformance_suite() {
    let container = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let port = container.get_host_port_ipv4(8123).await.expect("http port");
    let store = ChStore::new(&format!("http://127.0.0.1:{port}"));
    store.migrate().await.expect("migrate");

    conformance::run_all_scenarios(&store).await;
}
