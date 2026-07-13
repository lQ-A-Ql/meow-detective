use ceph_wire::{
    crc32c::ceph_crc32c, decode_bluefs_transaction, inspect_bluefs_transaction, CephEncode,
    CephStructEnvelope, CephWireError, BLUEFS_MAX_OPERATIONS,
};
use uuid::Uuid;

fn encode_string(value: &str, output: &mut Vec<u8>) {
    (value.len() as u32).encode(output);
    output.extend_from_slice(value.as_bytes());
}

fn encode_envelope(version: u8, compat: u8, payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    CephStructEnvelope {
        version,
        compat_version: compat,
        payload_length: payload.len() as u32,
    }
    .encode(&mut output);
    output.extend_from_slice(payload);
    output
}

fn encode_transaction(sequence: u64, operations: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    Uuid::parse_str("394d12df-4023-44dc-b4c5-10b5e5dd48f4")
        .unwrap()
        .encode(&mut payload);
    sequence.encode(&mut payload);
    (operations.len() as u32).encode(&mut payload);
    payload.extend_from_slice(operations);
    ceph_crc32c(operations).encode(&mut payload);
    encode_envelope(1, 1, &payload)
}

#[test]
fn inspects_and_decodes_transaction_framing_and_crc() {
    let transaction = encode_transaction(1, &[1]);
    let prefix = inspect_bluefs_transaction(&transaction).unwrap();
    assert_eq!(prefix.sequence, 1);
    assert_eq!(prefix.encoded_length, transaction.len());

    let decoded = decode_bluefs_transaction(&transaction).unwrap();
    assert_eq!(decoded.sequence, 1);
    assert_eq!(decoded.operations.len(), 1);
    assert_eq!(decoded.operation_crc32c, ceph_crc32c(&[1]));

    let mut corrupt = transaction;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(matches!(
        decode_bluefs_transaction(&corrupt),
        Err(CephWireError::BluefsTransactionCrcMismatch { .. })
    ));
}

#[test]
fn decodes_directory_jump_and_legacy_operations() {
    let mut operations = vec![1, 6];
    encode_string("db", &mut operations);
    operations.push(4);
    encode_string("db", &mut operations);
    encode_string("CURRENT", &mut operations);
    9u64.encode(&mut operations);
    operations.push(5);
    encode_string("db", &mut operations);
    encode_string("CURRENT", &mut operations);
    operations.push(7);
    encode_string("db", &mut operations);
    operations.push(2);
    1u8.encode(&mut operations);
    4096u64.encode(&mut operations);
    8192u64.encode(&mut operations);
    operations.push(3);
    1u8.encode(&mut operations);
    4096u64.encode(&mut operations);
    8192u64.encode(&mut operations);
    operations.push(10);
    100u64.encode(&mut operations);
    65_536u64.encode(&mut operations);
    operations.push(11);
    200u64.encode(&mut operations);
    operations.push(9);
    9u64.encode(&mut operations);

    let decoded = decode_bluefs_transaction(&encode_transaction(1, &operations)).unwrap();
    assert_eq!(decoded.operations.len(), 10);
}

#[test]
fn rejects_unknown_operation_and_oversized_payload_before_allocation() {
    for opcode in [0, 255] {
        let unknown = encode_transaction(1, &[opcode]);
        assert_eq!(
            decode_bluefs_transaction(&unknown).unwrap_err(),
            CephWireError::UnknownBluefsOperation { opcode }
        );
    }

    let mut payload = Vec::new();
    Uuid::nil().encode(&mut payload);
    1u64.encode(&mut payload);
    ((16 * 1024 * 1024 + 1) as u32).encode(&mut payload);
    let oversized = encode_envelope(1, 1, &payload);
    assert!(matches!(
        decode_bluefs_transaction(&oversized),
        Err(CephWireError::BluefsTransactionLengthLimit { .. })
    ));
}

#[test]
fn inspects_multiblock_transaction_from_first_block_only() {
    let operations = vec![1; 8192];
    let transaction = encode_transaction(7, &operations);

    let prefix = inspect_bluefs_transaction(&transaction[..4096]).unwrap();

    assert_eq!(prefix.sequence, 7);
    assert_eq!(prefix.encoded_length, transaction.len());
}

#[test]
fn rejects_operation_length_outside_declared_payload() {
    let mut payload = Vec::new();
    Uuid::nil().encode(&mut payload);
    1u64.encode(&mut payload);
    4096u32.encode(&mut payload);
    let invalid = encode_envelope(1, 1, &payload);

    assert!(matches!(
        inspect_bluefs_transaction(&invalid),
        Err(CephWireError::BluefsTransactionPayloadLengthMismatch { .. })
    ));
}

#[test]
fn rejects_excessive_operation_count_before_state_expansion() {
    let operations = vec![1; BLUEFS_MAX_OPERATIONS + 1];
    let transaction = encode_transaction(1, &operations);

    assert!(matches!(
        decode_bluefs_transaction(&transaction),
        Err(CephWireError::LengthLimit {
            context: "BlueFS transaction operations",
            length,
            limit: BLUEFS_MAX_OPERATIONS,
        }) if length == BLUEFS_MAX_OPERATIONS + 1
    ));
}
