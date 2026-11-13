use super::*;
use std::fs;
use transport::dto::AnalysisWorkProgressDto;

fn progress(
    stage: AnalysisWorkStageDto,
    records: u64,
    complete: bool,
) -> AnalysisExtractionProgressDto {
    AnalysisExtractionProgressDto {
        run_id: "progress-run".into(),
        case_id: "progress-case".into(),
        data_source_id: "source".into(),
        category: "Registry".into(),
        label: "Registry".into(),
        phase: AnalysisExtractionPhaseDto::Preparing,
        total_candidates: 1,
        processed_candidates: 0,
        structured_candidates: 1,
        unsupported_candidates: 0,
        text_fallback_candidates: 0,
        warning_candidates: 0,
        checkpoint_hit_count: 0,
        artifact_count: 0,
        timeline_event_count: 0,
        current_path: None,
        detail: String::new(),
        work: Some(AnalysisWorkProgressDto {
            stage,
            elapsed_ms: records,
            records,
            bytes: records * 10,
            complete,
        }),
    }
}

#[test]
fn preparing_work_changes_and_completion_are_persisted_with_throttled_updates() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/import");
    fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let profiler = ImportProfiler::with_output_directory(root.path()).unwrap();
    let mut observer =
        AnalysisAuditProgress::new(None, "progress-run", Instant::now(), Some(&profiler));
    observer.emit(progress(AnalysisWorkStageDto::GenerationVerify, 1, false));
    observer.emit(progress(AnalysisWorkStageDto::GenerationClone, 2, false));
    observer.emit(progress(AnalysisWorkStageDto::GenerationClone, 3, false));
    observer.emit(progress(AnalysisWorkStageDto::GenerationClone, 4, true));
    let session = fs::read_dir(root.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut records = fs::read_dir(session)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-progress.json")
        })
        .map(|path| {
            serde_json::from_slice::<AnalysisExtractionProgressDto>(&fs::read(path).unwrap())
                .unwrap()
        })
        .map(|progress| progress.work.unwrap().records)
        .collect::<Vec<_>>();
    records.sort();
    assert_eq!(records, [1, 2, 4]);
}
