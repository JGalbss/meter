//! Generate Rust types + gRPC service stubs from the engine proto contract (`proto/`) via tonic-build.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "../../proto/meter/v1/common.proto",
        "../../proto/meter/v1/ledger.proto",
        "../../proto/meter/v1/ingest.proto",
        "../../proto/meter/v1/query.proto",
        "../../proto/meter/v1/config.proto",
    ];
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&protos, &["../../proto"])?;
    for proto in protos {
        println!("cargo:rerun-if-changed={proto}");
    }
    Ok(())
}
