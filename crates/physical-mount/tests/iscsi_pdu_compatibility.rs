use iscsi_target::pdu::{flags, scsi_status, IscsiPdu};

#[test]
fn data_in_residual_flags_match_rfc_7143() {
    let mut underflow = data_in_pdu();
    underflow.set_data_in_residual(255, 8);
    assert_eq!(underflow.flags & flags::DATA_IN_UNDERFLOW, 0x02);
    assert_eq!(underflow.flags & flags::DATA_IN_OVERFLOW, 0);
    assert_eq!(residual_count(&underflow), 247);
    let wire = underflow.to_bytes();
    assert_eq!(wire[1], 0x83);
    assert_eq!(wire[3], scsi_status::GOOD);
    assert_eq!(u32::from_be_bytes(wire[44..48].try_into().unwrap()), 247);

    let mut overflow = data_in_pdu();
    overflow.set_data_in_residual(8, 255);
    assert_eq!(overflow.flags & flags::DATA_IN_OVERFLOW, 0x04);
    assert_eq!(overflow.flags & flags::DATA_IN_UNDERFLOW, 0);
    assert_eq!(residual_count(&overflow), 247);
}

fn data_in_pdu() -> IscsiPdu {
    IscsiPdu::scsi_data_in(
        1,
        u32::MAX,
        1,
        1,
        1,
        0,
        0,
        vec![0; 8],
        true,
        Some(scsi_status::GOOD),
    )
}

fn residual_count(pdu: &IscsiPdu) -> u32 {
    u32::from_be_bytes(pdu.specific[24..28].try_into().expect("residual bytes"))
}
