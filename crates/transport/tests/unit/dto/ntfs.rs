use crate::dto::{ForensicDataRunDto, NtfsTechnicalFileDto};

#[test]
fn forensic_run_uses_camel_case_contract() {
    let run = ForensicDataRunDto {
        header: 0x21,
        length_field_size: 1,
        offset_field_size: 2,
        cluster_count: 3,
        relative_lcn: Some(-2),
        absolute_lcn: Some(8),
        logical_offset: 0,
        raw: vec![0x21, 3, 8],
        physical_offset: Some(4096),
    };
    let value = serde_json::to_value(run).expect("serialize");
    assert_eq!(value["lengthFieldSize"], 1);
    assert_eq!(value["offsetFieldSize"], 2);
    assert_eq!(value["relativeLcn"], -2);
}

#[test]
fn technical_file_round_trips_empty_attributes() {
    let file = NtfsTechnicalFileDto {
        inode: 5,
        sequence_number: 1,
        flags: 3,
        parent_reference: None,
        record_offset: 4096,
        record_size: 1024,
        record_raw: vec![0x46, 0x49, 0x4c, 0x45],
        attributes: Vec::new(),
    };
    let round_trip: NtfsTechnicalFileDto =
        serde_json::from_value(serde_json::to_value(file.clone()).expect("serialize"))
            .expect("deserialize");
    assert_eq!(round_trip, file);
}
