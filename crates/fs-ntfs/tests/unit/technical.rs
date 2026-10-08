#[test]
fn forensic_run_preserves_header_fields_and_offsets() {
    let runs = crate::data_runs::parse_data_runs_forensic(&[0x21, 0x03, 0x10, 0x00, 0], 4096, 8192)
        .expect("run list");
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.header, 0x21);
    assert_eq!(run.length_field_size, 1);
    assert_eq!(run.offset_field_size, 2);
    assert_eq!(run.cluster_count, 3);
    assert_eq!(run.relative_lcn, Some(0x10));
    assert_eq!(run.absolute_lcn, Some(0x10));
    assert_eq!(run.logical_offset, 0);
    assert_eq!(run.raw, vec![0x21, 0x03, 0x10, 0x00]);
    assert_eq!(run.physical_offset, Some(8192 + 0x10 * 4096));
}

#[test]
fn forensic_run_accumulates_signed_lcn_delta() {
    let runs =
        crate::data_runs::parse_data_runs_forensic(&[0x11, 0x01, 0x0a, 0x11, 0x01, 0xf8, 0], 1, 0)
            .expect("run list");
    assert_eq!(runs[0].absolute_lcn, Some(10));
    assert_eq!(runs[1].relative_lcn, Some(-8));
    assert_eq!(runs[1].absolute_lcn, Some(2));
    assert_eq!(runs[1].logical_offset, 1);
}

#[test]
fn forensic_run_rejects_truncated_body() {
    let error = crate::data_runs::parse_data_runs_forensic(&[0x21, 0x01], 4096, 0)
        .expect_err("truncated run");
    assert!(error.to_string().contains("truncated"));
}
