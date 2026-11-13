mod artifact;
mod clone_store;
mod config;
mod core_scenarios;
mod correlation;
mod headers;
mod memory;
mod metrics;
mod native_fixture;
mod native_records;
mod native_scenarios;
mod scenarios;
mod search;
mod timeline;

#[test]
#[ignore = "explicit performance benchmark; run through scripts/run-benchmark.ps1"]
fn bench_all_scenarios() {
    let output = serde_json::json!({
        "schemaVersion": 2,
        "benchmarkVersion": config::Catalog::load().harness_version,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "platform": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "buildProfile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "scenarios": scenarios::run_all_scenarios(),
    });
    eprintln!("[BENCH-OUTPUT] {}", serde_json::to_string(&output).unwrap());
}
