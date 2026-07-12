use super::*;

#[test]
fn test_sanitize_csv_cell_normal() {
    assert_eq!(sanitize_csv_cell("hello"), "hello");
}

#[test]
fn test_sanitize_csv_cell_formula() {
    assert_eq!(sanitize_csv_cell("=SUM(A1:A2)"), "\t=SUM(A1:A2)");
    assert_eq!(sanitize_csv_cell("+cmd"), "\t+cmd");
    assert_eq!(sanitize_csv_cell("-DANGEROUS"), "\t-DANGEROUS");
    assert_eq!(sanitize_csv_cell("@SUM"), "\t@SUM");
}

#[test]
fn test_export_artifacts() {
    let mut output = Vec::new();
    let headers = vec!["Name", "Value"];
    let rows = vec![
        vec!["test".to_string(), "123".to_string()],
        vec!["hello".to_string(), "world".to_string()],
    ];

    CsvExporter::export_artifacts(&mut output, &headers, &rows).unwrap();
    let result = String::from_utf8(output).unwrap();
    assert!(result.contains("Name,Value"));
    assert!(result.contains("\"test\""));
}

#[test]
fn test_export_correlation_leads() {
    let mut output = Vec::new();
    let rows = vec![
        vec![
            "lead:1".to_string(),
            "cmd.exe 形成关联线索".to_string(),
            "direct".to_string(),
            "LNK; Prefetch".to_string(),
            "file-1".to_string(),
            "2".to_string(),
            "3".to_string(),
            "artifact:artifact-1:LNK; timeline:timeline-1:FILE_MODIFIED".to_string(),
            "路径类匹配仍需回跳原始工件复核。".to_string(),
        ],
        vec![
            "lead:2".to_string(),
            "svchost.exe 网络活动线索".to_string(),
            "strong".to_string(),
            "EVTX".to_string(),
            "file-2".to_string(),
            "0".to_string(),
            "1".to_string(),
            "".to_string(),
            "".to_string(),
        ],
    ];

    CsvExporter::export_correlation_leads(&mut output, &rows).unwrap();
    let result = String::from_utf8(output).unwrap();

    assert!(result.contains("lead_id,title,confidence,families,primary_file_path,supporting_node_count,match_signals_count,provenance_sources,caveats"));
    assert!(result.contains("\"lead:1\""));
    assert!(result.contains("\"cmd.exe 形成关联线索\""));
    assert!(result.contains("\"direct\""));
    assert!(result.contains("\"LNK; Prefetch\""));
    assert!(result.contains("\"file-1\""));
    assert!(result.contains("\"2\""));
    assert!(result.contains("\"3\""));
    assert!(result.contains("artifact:artifact-1:LNK; timeline:timeline-1:FILE_MODIFIED"));
    assert!(result.contains("路径类匹配仍需回跳原始工件复核。"));
    assert!(result.contains("\"lead:2\""));
    assert!(result.contains("\"strong\""));
    // Empty provenance and caveats should still appear as quoted empty strings
    assert!(result.contains("\"\""));
}

#[test]
fn test_export_correlation_leads_sanitizes_formula() {
    let mut output = Vec::new();
    let rows = vec![vec![
        "=LEAD_ID".to_string(),
        "=CMD.EXE".to_string(),
        "direct".to_string(),
        "-DANGEROUS".to_string(),
        "+file-1".to_string(),
        "0".to_string(),
        "0".to_string(),
        "@SUM".to_string(),
        "".to_string(),
    ]];

    CsvExporter::export_correlation_leads(&mut output, &rows).unwrap();
    let result = String::from_utf8(output).unwrap();

    assert!(result.contains("\"\t=LEAD_ID\""));
    assert!(result.contains("\"\t=CMD.EXE\""));
    assert!(result.contains("\"\t-DANGEROUS\""));
    assert!(result.contains("\"\t+file-1\""));
    assert!(result.contains("\"\t@SUM\""));
}
