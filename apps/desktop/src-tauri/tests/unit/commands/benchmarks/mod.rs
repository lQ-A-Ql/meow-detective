mod fixture;
mod metrics;
mod scenarios;

#[test]
fn bench_all_scenarios() {
    let output = serde_json::json!({
        "benchmarkVersion": "2026.06",
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "hostProfile": "Windows 11 Pro / 32GB RAM / NVMe / Rust stable",
        "scenarios": scenarios::run_all_scenarios(),
    });
    eprintln!("[BENCH-OUTPUT] {}", serde_json::to_string(&output).unwrap());
}
