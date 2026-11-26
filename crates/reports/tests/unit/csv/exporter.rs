use super::*;

#[test]
fn csv_values_escape_formulas_quotes_and_newlines() {
    let mut output = Vec::new();
    CsvExporter::export_rows(
        &mut output,
        &["field", "value"],
        &[
            vec!["=SUM(A1:A2)".into(), "a\"b\nc".into()],
            vec!["@command".into(), "value".into()],
        ],
    )
    .unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("\"\t=SUM(A1:A2)\""));
    assert!(text.contains("\"a\"\"b\nc\""));
    assert!(text.contains("\"\t@command\""));
}
