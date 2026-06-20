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

    // The account id printed by `seed` drives `grant` and `balance`.
    let account = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("account "))
        .expect("seed prints the account id")
        .to_owned();

    let granted = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args([
            "grant",
            "--database-url",
            &url,
            "--account",
            &account,
            "--credits",
            "250",
        ])
        .output()
        .expect("run meterctl grant");
    assert!(
        granted.status.success(),
        "grant failed: {}",
        String::from_utf8_lossy(&granted.stderr)
    );

    let bal = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args(["balance", "--database-url", &url, "--account", &account])
        .output()
        .expect("run meterctl balance");
    assert!(
        bal.status.success(),
        "balance failed: {}",
        String::from_utf8_lossy(&bal.stderr)
    );
    let bal_out = String::from_utf8_lossy(&bal.stdout);
    // 500 seeded + 250 granted = 750 settled and available, nothing held.
    assert!(
        bal_out.contains("settled   750 credits"),
        "balance: {bal_out}"
    );
    assert!(
        bal_out.contains("available 750 credits"),
        "balance: {bal_out}"
    );
    assert!(
        bal_out.contains("held      0 credits"),
        "balance: {bal_out}"
    );
}
