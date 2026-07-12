use crate::header::{NID_ROOT_FOLDER, PST_MAGIC};
use crate::props::read_u16_le;
use crate::pst::PstReader;

const NID_MESSAGE_STORE: u32 = 0x21;

fn build_synthetic_unicode_pst() -> Vec<u8> {
    crate::tests::build_synthetic_unicode_pst()
}

#[test]
fn synthetic_pst_header_magic() {
    let pst = build_synthetic_unicode_pst();
    assert_eq!(&pst[0..4], &PST_MAGIC);
    assert_eq!(read_u16_le(&pst, 10).unwrap(), 23);
}

#[test]
fn open_synthetic_pst() {
    let pst = build_synthetic_unicode_pst();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.pst");
    std::fs::write(&path, &pst).unwrap();
    let reader = PstReader::open(&path).unwrap();
    assert!(reader.is_unicode());
    assert!(reader.file_size() > 0);
}

#[test]
fn synthetic_pst_nbt_entries() {
    let pst = build_synthetic_unicode_pst();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.pst");
    std::fs::write(&path, &pst).unwrap();
    let reader = PstReader::open(&path).unwrap();
    assert!(reader.nbt_cache.contains_key(&NID_MESSAGE_STORE));
    assert!(reader.nbt_cache.contains_key(&NID_ROOT_FOLDER));
}

#[test]
fn fold_and_read_nbt_structure() {
    let pst = build_synthetic_unicode_pst();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.pst");
    std::fs::write(&path, &pst).unwrap();
    let reader = PstReader::open(&path).unwrap();
    assert_eq!(reader.header.root_nbt.bid, 4);
    assert_eq!(reader.header.root_bbt.bid, 2);
}

#[test]
fn synthetic_pst_property_context() {
    let pst = build_synthetic_unicode_pst();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.pst");
    std::fs::write(&path, &pst).unwrap();
    let reader = PstReader::open(&path).unwrap();
    let block = reader
        .read_subnode_block(NID_MESSAGE_STORE)
        .expect("message store subnode");
    assert!(!reader.parse_property_context(block).is_empty());
}

#[test]
fn synthetic_pst_read_messages() {
    let pst = build_synthetic_unicode_pst();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.pst");
    std::fs::write(&path, &pst).unwrap();
    let reader = PstReader::open(&path).unwrap();
    let messages = reader.read_messages().unwrap();
    assert_eq!(messages.len(), 1, "expected one message, got {messages:?}");
    let msg = &messages[0];
    assert_eq!(msg.subject, "Synthetic Subject 1");
    assert_eq!(msg.sender_name, "Sender 1");
    assert_eq!(msg.sender_email, "sender1@example.com");
    assert!(msg
        .body_plain
        .contains("Body text for synthetic message number 1."));
}

#[test]
fn invalid_pst_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.pst");
    std::fs::write(&path, b"not a pst file").unwrap();
    assert!(PstReader::open(&path).is_err());
}

#[test]
fn empty_file_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.pst");
    std::fs::write(&path, []).unwrap();
    assert!(PstReader::open(&path).is_err());
}
