use super::*;
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::capability::find_capability;
use domain::{DataSourcePlatform, FileEntryId};

fn candidate(path: &str, id: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(id.to_string()),
        data_source_id: "source-linux".to_string(),
        partition_index: Some(2),
        path: path.to_string(),
        size: 32,
        content_identity: format!("identity-{id}"),
        evidence_kind: "linux_artifact".to_string(),
        parser: "linux.artifacts".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

#[test]
fn reports_real_candidate_inventory_and_monotonic_processing() {
    let capability = find_capability("LinuxJournal").expect("Linux journal capability");
    let candidates = [
        candidate("/var/log/journal/machine/system.journal", "structured"),
        candidate("/var/log/syslog", "fallback"),
        candidate("/var/lib/unknown-artifact", "unsupported"),
    ];
    let mut updates = Vec::new();
    let mut collect_update = |update| updates.push(update);
    let mut reporter = ExtractionProgressReporter::new(
        DataSourcePlatform::Linux,
        &[capability],
        &mut collect_update,
    );

    reporter.emit_discovering();
    for item in &candidates {
        reporter.register_candidate(capability, item);
    }
    reporter.emit_preparing();
    reporter.begin_extraction();
    for item in &candidates {
        reporter.start_candidate(capability, item);
        reporter.finish_candidate(
            capability,
            item,
            CandidateProgressResult {
                artifact_count: 1,
                ..CandidateProgressResult::default()
            },
        );
    }
    reporter.complete();
    drop(reporter);

    let extracting = updates
        .iter()
        .filter(|update| update.phase == AnalysisExtractionPhaseDto::Extracting)
        .collect::<Vec<_>>();
    assert!(extracting
        .windows(2)
        .all(|pair| { pair[0].processed_candidates <= pair[1].processed_candidates }));
    let completed = updates
        .iter()
        .find(|update| update.phase == AnalysisExtractionPhaseDto::Completed)
        .expect("completed progress event");
    assert_eq!(completed.total_candidates, 3);
    assert_eq!(completed.processed_candidates, 3);
    assert_eq!(completed.structured_candidates, 1);
    assert_eq!(completed.text_fallback_candidates, 1);
    assert_eq!(completed.unsupported_candidates, 1);
    assert!(updates.iter().any(|update| {
        update.current_path.as_deref() == Some("/var/log/syslog")
            && update.detail.contains("text-fallback")
    }));
    assert!(updates.iter().any(|update| {
        update.current_path.as_deref() == Some("/var/lib/unknown-artifact")
            && update.detail.contains("unsupported")
    }));
}
