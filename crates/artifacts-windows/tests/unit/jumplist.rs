use super::*;
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};

use crate::tests::parser_fixtures as fixtures;

#[test]
fn jump_list_supports_standard_names() {
    assert!(JumpListExtractor.supports_path("Recent/f01b4d95cf55d32a.automaticDestinations-ms"));
    assert!(JumpListExtractor.supports_path("Recent/custom.customDestinations-ms"));
    assert!(!JumpListExtractor.supports_path("Recent/5f7b5f7e3243a7b8.ms-abc"));
    assert!(!JumpListExtractor.supports_path("file.lnk"));
}

#[test]
fn jump_list_truncated_input_is_rejected() {
    let ctx = ArtifactContext {
        file_id: domain::FileEntryId("truncated".into()),
        file_path: "test.automaticDestinations-ms".into(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 10])),
    };
    let mut sink = VecSink::new();
    assert!(JumpListExtractor.run(ctx, &mut sink).is_err());
    assert!(sink.artifacts.is_empty());
}

#[test]
fn real_jump_lists_preserve_container_identity_and_format() {
    for (name, format, links) in [
        (
            "f01b4d95cf55d32a.automaticDestinations-ms",
            "AutomaticDestinations",
            9,
        ),
        (
            "368d807282ccde9d.customDestinations-ms",
            "CustomDestinations",
            3,
        ),
    ] {
        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("container-source".into()),
            file_path: name.into(),
            reader: Box::new(std::io::Cursor::new(fixtures::bytes(name))),
        };
        let mut sink = VecSink::new();
        let report = JumpListExtractor
            .run(ctx, &mut sink)
            .expect("decode public Jump List");
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.artifacts_found, links + 1);
        assert!(sink.artifacts.iter().all(|artifact| artifact
            .source_object_id
            .as_ref()
            .unwrap()
            .0
            == "container-source"));
        assert!(sink
            .timeline_events
            .iter()
            .all(|event| event.source_object_id == "container-source"));
        let summary = sink
            .artifacts
            .iter()
            .find(|artifact| artifact.family == "JumpList")
            .unwrap();
        assert_eq!(summary.attrs["format"], format);
        assert_eq!(summary.attrs["lnk_count"], links);
        assert!(sink
            .artifacts
            .iter()
            .filter(|artifact| artifact.family == "LNK")
            .all(|artifact| artifact.attrs.contains_key("containerEntry")));
    }
}
