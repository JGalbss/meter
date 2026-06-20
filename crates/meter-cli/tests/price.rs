//! Test for `meterctl price`: it prices catalog usage with no database, so it runs the real binary
//! directly (no container needed).

use std::process::Command;

#[test]
fn prices_a_catalog_model() {
    // gpt-5: 1000 input @ $1.25/M + 500 output @ $10/M = 0.00625 USD; at 1 micro-USD/credit -> 6250.
    let output = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args([
            "price", "--model", "gpt-5", "--input", "1000", "--output", "500",
        ])
        .output()
        .expect("run meterctl price");

    assert!(
        output.status.success(),
        "price failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0.00625"), "stdout: {stdout}");
    assert!(stdout.contains("credits 6250"), "stdout: {stdout}");
}

#[test]
fn unknown_model_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args(["price", "--model", "not-a-model", "--input", "10"])
        .output()
        .expect("run meterctl price");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown model"));
}
