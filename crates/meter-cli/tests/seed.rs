//! End-to-end test for `meterctl seed`: run the real binary against a real Postgres container and
//! assert it migrates, opens a funded account, and reports the balance.

use std::process::Command;

use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
async fn seed_creates_a_funded_account() {
    let postgres = Postgres::default().start().await.expect("start postgres");
    let port = postgres
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    // Run the actual compiled binary (Cargo exports its path for integration tests).
    let output = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args(["seed", "--database-url", &url, "--credits", "500"])
        .output()
        .expect("run meterctl seed");

    assert!(
        output.status.success(),
        "seed failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("seeded org"), "stdout: {stdout}");
    assert!(stdout.contains("balance 500 credits"), "stdout: {stdout}");

    // Seeding is idempotent at the migration layer: a second run still succeeds.
    let again = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args(["seed", "--database-url", &url, "--credits", "250"])
        .output()
        .expect("run meterctl seed again");
    assert!(
        again.status.success(),
        "second seed failed: {}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert!(String::from_utf8_lossy(&again.stdout).contains("balance 250 credits"));
}
