#[path = "support/mcp_files.rs"]
mod support;

use serde_json::json;
use support::Fixture;

#[test]
fn image_steg_tool_returns_bounded_triage_for_magic_detected_png() {
    let fixture = Fixture::new();
    let png_path = fixture.temporary.path().join("evidence/red.png");
    let before = std::fs::read(&png_path).unwrap();
    let report = fixture
        .call(
            "forensics.analyze_image_steg",
            json!({"fileId":fixture.id("red.png")}),
        )
        .unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["format"], "png");
    assert_eq!(report["width"], 1);
    assert_eq!(report["height"], 1);
    assert_eq!(report["blocksTotal"], 1);
    assert_eq!(report["blocksScored"], 0);
    assert!(report.get("global").is_none());
    assert_eq!(report["planes"].as_array().unwrap().len(), 3);
    assert_eq!(report["planes"][0]["plane"], "r");
    assert_eq!(report["planes"][0]["samples"], 1);
    assert!(report["planes"][0].get("assessment").is_none());
    let warnings = report["warnings"].as_array().unwrap();
    assert!(warnings
        .iter()
        .any(|warning| warning["code"] == "image.steg.small-image"));
    assert_eq!(std::fs::read(&png_path).unwrap(), before);
}

#[test]
fn image_steg_tool_reports_unsupported_for_non_image_magic() {
    let fixture = Fixture::new();
    let binary_path = fixture.temporary.path().join("evidence/binary.bin");
    assert_eq!(fixture.binary.len(), 130_001);
    let report = fixture
        .call(
            "forensics.analyze_image_steg",
            json!({"fileId":fixture.id("binary.bin")}),
        )
        .unwrap();
    assert_eq!(report["status"], "unsupported");
    assert_eq!(report["reason"], "image.format");
    assert!(report.get("format").is_none());
    assert!(report.get("planes").is_none());
    assert_eq!(std::fs::read(&binary_path).unwrap(), fixture.binary);

    let text = fixture
        .call(
            "forensics.analyze_image_steg",
            json!({"fileId":fixture.id("text.txt")}),
        )
        .unwrap();
    assert_eq!(text["status"], "unsupported");
}
