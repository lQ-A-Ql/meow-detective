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

#[test]
fn ordinary_and_forensic_views_agree_across_sparse_and_negative_deltas() {
    let bytes = [0x11, 2, 10, 0x01, 3, 0x11, 1, 0xf8, 0];
    let ordinary = crate::data_runs::parse_data_runs_ext(&bytes).unwrap();
    let forensic = crate::data_runs::parse_data_runs_forensic(&bytes, 4096, 8192).unwrap();
    assert_eq!(ordinary.len(), 3);
    for (light, detailed) in ordinary.iter().zip(&forensic) {
        assert_eq!(light.lcn, detailed.absolute_lcn);
        assert_eq!(light.cluster_count, detailed.cluster_count);
    }
    assert_eq!(ordinary[1].lcn, None);
    assert_eq!(ordinary[2].lcn, Some(2));
    assert_eq!(forensic[1].logical_offset, 8192);
    assert_eq!(forensic[2].logical_offset, 20480);
}

#[test]
fn both_views_fail_closed_on_invalid_headers_ranges_and_termination() {
    for bytes in [
        vec![0x10, 1, 0],
        vec![0x19, 1, 0],
        vec![0x91, 1, 0],
        vec![0x21, 1],
        vec![0x11, 0, 1, 0],
        vec![0x11, 1, 0xff, 0],
        vec![0x11, 1, 1],
    ] {
        assert!(
            crate::data_runs::parse_data_runs_ext(&bytes).is_err(),
            "{bytes:x?}"
        );
        assert!(
            crate::data_runs::parse_data_runs_forensic(&bytes, 1, 0).is_err(),
            "{bytes:x?}"
        );
    }
}

#[test]
fn both_views_reject_lcn_and_cluster_count_overflow() {
    let mut lcn_overflow = vec![0x81, 1];
    lcn_overflow.extend_from_slice(&i64::MAX.to_le_bytes());
    lcn_overflow.extend_from_slice(&[0x11, 1, 1, 0]);
    let mut count_overflow = vec![0x08];
    count_overflow.extend_from_slice(&u64::MAX.to_le_bytes());
    count_overflow.extend_from_slice(&[0x01, 1, 0]);
    for bytes in [lcn_overflow, count_overflow] {
        assert!(crate::data_runs::parse_data_runs_ext(&bytes).is_err());
        assert!(crate::data_runs::parse_data_runs_forensic(&bytes, 1, 0).is_err());
    }
}

#[test]
fn forensic_view_rejects_unrepresentable_physical_and_logical_ranges() {
    assert!(crate::data_runs::parse_data_runs_forensic(&[0x11, 1, 1, 0], 1, u64::MAX).is_err());
    assert!(crate::data_runs::parse_data_runs_forensic(&[0x01, 2, 0], u64::MAX, 0).is_err());
    assert!(crate::data_runs::parse_data_runs_forensic(&[0], 0, 0).is_err());
}

#[test]
fn both_views_enforce_the_same_run_count_limit() {
    let mut bytes = [0x01, 1].repeat(100_001);
    bytes.push(0);
    assert!(crate::data_runs::parse_data_runs_ext(&bytes).is_err());
    assert!(crate::data_runs::parse_data_runs_forensic(&bytes, 1, 0).is_err());
}
