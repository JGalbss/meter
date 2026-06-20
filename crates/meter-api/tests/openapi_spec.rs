//! The engine OpenAPI document is committed to `openapi.json` and drift-checked, so the contract can't
//! silently diverge from the code (the engine counterpart to the control-plane's `openapi:emit` gate).
//! Regenerate after an intentional API change with:
//!   `METER_OPENAPI_BLESS=1 cargo test -p meter-api --test openapi_spec`

use std::path::PathBuf;

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("openapi.json")
}

#[test]
fn openapi_json_is_committed_and_current() {
    let doc = meter_api::openapi_document();
    let current = format!(
        "{}\n",
        serde_json::to_string_pretty(&doc).expect("serialize openapi")
    );
    let path = spec_path();

    if std::env::var("METER_OPENAPI_BLESS").is_ok() {
        std::fs::write(&path, &current).expect("write openapi.json");
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "openapi.json missing — regenerate with \
             `METER_OPENAPI_BLESS=1 cargo test -p meter-api --test openapi_spec`"
        )
    });
    assert!(
        committed == current,
        "openapi.json is stale — regenerate with \
         `METER_OPENAPI_BLESS=1 cargo test -p meter-api --test openapi_spec`"
    );
}
