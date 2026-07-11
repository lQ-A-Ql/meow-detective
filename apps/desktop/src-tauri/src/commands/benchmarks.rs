//! Benchmark tests that output parseable timing data for the benchmark harness.
//!
//! Each test function sets up a fresh case, imports logical evidence, and runs
//! the target operation multiple times. Results are emitted as JSON on stderr
//! so `scripts/run-benchmark.ps1` can collect them.
//!
//! Output line format (one per test run):
//!   [BENCH-OUTPUT] {"scenarios":[...],"benchmarkVersion":"2026.06",...}

#[cfg(test)]
mod benchmark_tests {
    use app_services::{case_service, search_service, timeline_service};
    use chrono::Utc;
    use domain::DataSourceId;
    use persistence_sqlite::repositories::{
        artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
        job_repo::JobRepo,
    };
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::Instant;
    use tempfile::TempDir;

    use crate::commands::import::pipeline::{execute_import_job, ImportJobOptions};

    fn prefetch_fixture(
        exe_name: &str,
        run_count: u32,
        last_run: chrono::DateTime<Utc>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        // SCCA header (Prefetch file signature)
        data.extend_from_slice(&0x1Eu32.to_le_bytes());
        data.extend_from_slice(b"SCCA");
        data.extend_from_slice(&0x11u32.to_le_bytes());
        data.extend_from_slice(&0x0000A000u32.to_le_bytes());

        let mut name_buf = vec![0u8; 60];
        for (index, ch) in exe_name.encode_utf16().enumerate() {
            let offset = index * 2;
            if offset + 1 < name_buf.len() {
                name_buf[offset] = (ch & 0xFF) as u8;
                name_buf[offset + 1] = (ch >> 8) as u8;
            }
        }
        data.extend_from_slice(&name_buf);
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let filetime = |dt: chrono::DateTime<Utc>| -> u64 {
            ((dt.timestamp() + 11_644_473_600) as u64 * 10_000_000)
                + (dt.timestamp_subsec_nanos() as u64 / 100)
        };
        let mut file_info = vec![0u8; 212];
        file_info[0..4].copy_from_slice(&0x128u32.to_le_bytes());
        file_info[8..12].copy_from_slice(&0x128u32.to_le_bytes());
        file_info[16..20].copy_from_slice(&0x128u32.to_le_bytes());
        file_info[24..28].copy_from_slice(&0x128u32.to_le_bytes());
        file_info[44..52].copy_from_slice(&filetime(last_run).to_le_bytes());
        file_info[116..120].copy_from_slice(&run_count.to_le_bytes());
        file_info[120..124].copy_from_slice(&1u32.to_le_bytes());
        file_info[124..128].copy_from_slice(&3u32.to_le_bytes());
        file_info[128..132].copy_from_slice(&0x128u32.to_le_bytes());
        data.extend_from_slice(&file_info);

        data.resize(4096, 0);
        data
    }

    /// Helper: set up a case, import evidence, return (active_case, _tempdir).
    fn setup_case() -> (app_services::active_case::ActiveCase, TempDir) {
        let tmp = TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();

        let marker = "fw_bench_search_a1b2c3";
        std::fs::write(
            evidence_dir.join("notes.txt"),
            format!("Forensics import marker: {marker}"),
        )
        .unwrap();
        std::fs::write(
            evidence_dir.join("system-log.txt"),
            "System log: boot at 2026-01-15, user alice logged in at 2026-01-15T08:00:00Z\n",
        )
        .unwrap();
        // Two prefetch fixtures
        std::fs::write(
            evidence_dir.join("CMD.EXE-DEADBEEF.pf"),
            prefetch_fixture("CMD.EXE", 3, Utc::now()),
        )
        .unwrap();
        std::fs::write(
            evidence_dir.join("NOTEPAD.EXE-12345678.pf"),
            prefetch_fixture("NOTEPAD.EXE", 1, Utc::now()),
        )
        .unwrap();
        let active = case_service::create_case(
            &tmp.path().join("cases"),
            "bench-case",
            Some("benchmark-runner"),
        )
        .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        active
            .with_conn(|conn| {
                let job_id = JobRepo::new(conn)
                    .create(&active.meta.id.0, "Benchmark import")
                    .unwrap();
                let import_config =
                    app_services::import_precheck::prepare_import_source_config_from_path(
                        &evidence_dir.to_string_lossy(),
                        domain::DataSourcePlatform::Windows,
                    )
                    .unwrap();
                execute_import_job(
                    conn,
                    &active.meta.id,
                    &active.case_root,
                    import_config,
                    &job_id,
                    ImportJobOptions {
                        event_sink: None,
                        cancel_token: &cancel,
                        max_import_workers: None,
                        max_analysis_workers: Some(1),
                        analysis_mode:
                            app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
                    },
                )
                .expect("benchmark setup import should succeed");
                Ok(())
            })
            .unwrap();

        (active, tmp)
    }

    /// Run an operation `runs` times, return Vec of elapsed_ms per run.
    fn measure_runs(runs: u32, mut op: impl FnMut()) -> Vec<u64> {
        let mut elapsed = Vec::with_capacity(runs as usize);
        for _ in 0..runs {
            let started = Instant::now();
            op();
            elapsed.push(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        }
        elapsed
    }

    /// Compute p95 from sorted timings.
    fn p95_ms(mut timings: Vec<u64>) -> u64 {
        if timings.is_empty() {
            return 0;
        }
        timings.sort_unstable();
        let index = ((timings.len() as f64) * 0.95).ceil() as usize;
        timings[index.saturating_sub(1)]
    }

    /// Estimated peak memory in MB (best-effort via RSS snapshot).
    fn peak_memory_mb() -> u64 {
        app_services::import_analysis::current_rss_mb()
    }

    fn first_data_source_id(conn: &rusqlite::Connection, case_id: &domain::CaseId) -> DataSourceId {
        DataSourceRepo::new(conn)
            .find_by_case(case_id)
            .unwrap()
            .into_iter()
            .next()
            .expect("benchmark import should register a data source")
            .id
    }

    #[test]
    fn bench_all_scenarios() {
        let warmup_runs = 2u32;
        let measure_runs_count = 5u32;
        let (active, _tmp) = setup_case();
        let data_source_id = active
            .with_conn(|conn| Ok(first_data_source_id(conn, &active.meta.id)))
            .unwrap();
        let source_conn =
            app_services::source_db::open_source_db(&active.case_root, &data_source_id)
                .expect("benchmark source DB should exist");
        let source_index_dir =
            app_services::source_db::source_index_dir(&active.case_root, &data_source_id);

        // Verify setup: file entries and artifacts exist
        let fc = FileRepo::new(&source_conn).count_all().unwrap();
        assert!(fc >= 2, "Expected file entries, got {fc}");

        let ac = ArtifactRepo::new(&source_conn).count().unwrap();
        assert!(ac >= 1, "Expected at least 1 artifact, got {ac}");

        let marker = "fw_bench_search_a1b2c3";
        let mut results: Vec<serde_json::Value> = Vec::new();

        // ── search_query ──────────────────────────────────────────
        {
            for _ in 0..warmup_runs {
                let _ = search_service::search_files_real(&source_index_dir, marker, 0, 10);
            }

            let timings = measure_runs(measure_runs_count, || {
                let _ = search_service::search_files_real(&source_index_dir, marker, 0, 10);
            });

            results.push(serde_json::json!({
                "scenario": "search_query",
                "datasetLevel": "small",
                "p95Ms": p95_ms(timings.clone()),
                "memoryPeakMb": peak_memory_mb(),
                "runs": timings.iter().map(|t| serde_json::json!({"elapsedMs": t})).collect::<Vec<_>>(),
            }));
        }

        // ── file_tree_expand ──────────────────────────────────────
        {
            for _ in 0..warmup_runs {
                let _ = app_services::file_service::get_file_tree_real(&source_conn);
            }

            let timings = measure_runs(measure_runs_count, || {
                let _ = app_services::file_service::get_file_tree_real(&source_conn);
            });

            results.push(serde_json::json!({
                "scenario": "file_tree_expand",
                "datasetLevel": "small",
                "p95Ms": p95_ms(timings.clone()),
                "memoryPeakMb": peak_memory_mb(),
                "runs": timings.iter().map(|t| serde_json::json!({"elapsedMs": t})).collect::<Vec<_>>(),
            }));
        }

        // ── file_paginate ─────────────────────────────────────────
        {
            for _ in 0..warmup_runs {
                let _ = FileRepo::new(&source_conn).find_root_entries_page(0, 10);
            }

            let timings = measure_runs(measure_runs_count, || {
                let _ = FileRepo::new(&source_conn).find_root_entries_page(0, 10);
            });

            results.push(serde_json::json!({
                "scenario": "file_paginate",
                "datasetLevel": "small",
                "p95Ms": p95_ms(timings.clone()),
                "memoryPeakMb": peak_memory_mb(),
                "runs": timings.iter().map(|t| serde_json::json!({"elapsedMs": t})).collect::<Vec<_>>(),
            }));
        }

        // ── timeline_filter ───────────────────────────────────────
        {
            // Ensure timeline is projected (lazy on metadata-only import)
            let _ = timeline_service::ensure_macb_timeline_projected(&source_conn);

            for _ in 0..warmup_runs {
                let _ = timeline_service::query_timeline(&source_conn, 0, 20);
            }

            let timings = measure_runs(measure_runs_count, || {
                let _ = timeline_service::query_timeline(&source_conn, 0, 20);
            });

            results.push(serde_json::json!({
                "scenario": "timeline_filter",
                "datasetLevel": "small",
                "p95Ms": p95_ms(timings.clone()),
                "memoryPeakMb": peak_memory_mb(),
                "runs": timings.iter().map(|t| serde_json::json!({"elapsedMs": t})).collect::<Vec<_>>(),
            }));
        }

        // ── artifact_extract ──────────────────────────────────────
        {
            let repo = ArtifactRepo::new(&source_conn);
            for _ in 0..warmup_runs {
                let _ = repo.list_by_family(None);
            }

            let timings = measure_runs(measure_runs_count, || {
                let _ = repo.list_by_family(None);
            });

            results.push(serde_json::json!({
                "scenario": "artifact_extract",
                "datasetLevel": "small",
                "p95Ms": p95_ms(timings.clone()),
                "memoryPeakMb": peak_memory_mb(),
                "runs": timings.iter().map(|t| serde_json::json!({"elapsedMs": t})).collect::<Vec<_>>(),
            }));
        }

        // ── report_export ─────────────────────────────────────────
        {
            let output_dir = _tmp.path().join("reports");
            std::fs::create_dir_all(&output_dir).unwrap();

            let scope = transport::commands::ExportScopeDto {
                file_system_metadata: true,
                registry: true,
                full_timeline: true,
                raw_file_extraction: false,
                overwrite: true,
            };

            active
                .with_conn(|conn| {
                    for _ in 0..warmup_runs {
                        let _ = app_services::report::generate_html_report(
                            conn,
                            &active.meta,
                            &output_dir,
                            &scope,
                        );
                    }
                    Ok(())
                })
                .unwrap();

            let timings = active
                .with_conn(|conn| {
                    Ok(measure_runs(measure_runs_count, || {
                        let _ = app_services::report::generate_html_report(
                            conn,
                            &active.meta,
                            &output_dir,
                            &scope,
                        );
                    }))
                })
                .unwrap();

            results.push(serde_json::json!({
                "scenario": "report_export",
                "datasetLevel": "small",
                "p95Ms": p95_ms(timings.clone()),
                "memoryPeakMb": peak_memory_mb(),
                "runs": timings.iter().map(|t| serde_json::json!({"elapsedMs": t})).collect::<Vec<_>>(),
            }));
        }

        // ── Emit result ───────────────────────────────────────────
        let output = serde_json::json!({
            "benchmarkVersion": "2026.06",
            "generatedAt": chrono::Utc::now().to_rfc3339(),
            "hostProfile": "Windows 11 Pro / 32GB RAM / NVMe / Rust stable",
            "scenarios": results,
        });
        eprintln!("[BENCH-OUTPUT] {}", serde_json::to_string(&output).unwrap());
    }
}
