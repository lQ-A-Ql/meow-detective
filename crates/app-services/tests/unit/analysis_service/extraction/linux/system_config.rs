use super::*;

fn test_candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: domain::FileEntryId("file-systemd-unit".to_string()),
        data_source_id: "ds-test".to_string(),
        partition_index: None,
        path: path.to_string(),
        size: 4096,
        encrypted: false,
        content_identity: "test:systemd-unit".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "test".to_string(),
        parser: "test".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

fn artifact_lines(outcome: &ExtractionOutcome) -> Vec<&str> {
    outcome
        .artifacts
        .iter()
        .filter_map(|artifact| artifact.attrs.get("line").and_then(Value::as_str))
        .collect()
}

#[test]
fn systemd_unit_extraction_skips_section_headers() {
    let candidate = test_candidate("etc/systemd/system/demo.service");
    let unit = "[Unit]\n\
                Description=Demo service\n\
                After=network.target\n\
                \n\
                [Service]\n\
                ExecStart=/usr/bin/demo --serve\n\
                # Restart=on-failure\n\
                \n\
                [Install]\n\
                WantedBy=multi-user.target\n";
    let mut outcome = ExtractionOutcome::default();

    extract_systemd_unit_config(&candidate, unit.as_bytes(), &mut outcome);

    let lines = artifact_lines(&outcome);
    assert_eq!(
        lines,
        vec![
            "Description=Demo service",
            "After=network.target",
            "ExecStart=/usr/bin/demo --serve",
            "WantedBy=multi-user.target",
        ],
        "section headers and comments must not produce records"
    );
    assert!(outcome
        .artifacts
        .iter()
        .all(|artifact| artifact.attrs.contains_key("key")));
    assert!(outcome
        .artifacts
        .iter()
        .all(|artifact| artifact.extractor_id.as_deref() == Some("linux.systemd_unit")));
    assert!(outcome.warnings.is_empty());
}

#[test]
fn systemd_unit_extraction_preserves_line_numbers() {
    let candidate = test_candidate("usr/lib/systemd/system/demo.service");
    let unit = "[Unit]\nDescription=Demo service\n[Service]\nExecStart=/usr/bin/demo\n";
    let mut outcome = ExtractionOutcome::default();

    extract_systemd_unit_config(&candidate, unit.as_bytes(), &mut outcome);

    let line_numbers: Vec<u64> = outcome
        .artifacts
        .iter()
        .filter_map(|artifact| artifact.attrs.get("lineNumber").and_then(Value::as_u64))
        .collect();
    assert_eq!(
        line_numbers,
        vec![2, 4],
        "skipped section headers must not renumber real records"
    );
}

#[test]
fn generic_text_config_keeps_section_like_lines() {
    let candidate = test_candidate("etc/sudoers.d/demo");
    let text = "[custom]\nroot ALL=(ALL) ALL\n";
    let mut outcome = ExtractionOutcome::default();

    extract_text_config(
        &candidate,
        text.as_bytes(),
        "linux.sudoers",
        "sudoers",
        &mut outcome,
    );

    let lines = artifact_lines(&outcome);
    assert_eq!(
        lines,
        vec!["[custom]", "root ALL=(ALL) ALL"],
        "non-systemd text configs must keep bracket lines"
    );
}

#[test]
fn ini_section_header_detection_matches_trimmed_shape() {
    assert!(is_ini_section_header("[Unit]"));
    assert!(is_ini_section_header("[]"));
    assert!(!is_ini_section_header("[Unit] extra"));
    assert!(!is_ini_section_header("Description=[Unit]"));
    assert!(!is_ini_section_header("[Unit"));
    assert!(!is_ini_section_header("Unit]"));
    assert!(!is_ini_section_header("[a]b[c]"));
}
