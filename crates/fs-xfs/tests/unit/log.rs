use super::record::lh_off;
use super::recovery::{extract_dirent_from_buf, parse_logged_inode_core};
use super::*;

fn build_log_record_header(cycle: u16, version: u16, rec_len: u16) -> Vec<u8> {
    let mut header = vec![0u8; XLOG_REC_HEADER_SIZE];
    header[lh_off::MAGIC..lh_off::MAGIC + 2].copy_from_slice(&XLOG_HEADER_MAGIC.to_be_bytes());
    header[lh_off::CYCLE..lh_off::CYCLE + 2].copy_from_slice(&cycle.to_be_bytes());
    header[lh_off::VERSION..lh_off::VERSION + 2].copy_from_slice(&version.to_be_bytes());
    header[lh_off::LEN..lh_off::LEN + 2].copy_from_slice(&rec_len.to_be_bytes());
    header
}

fn build_log_record(items: &[Vec<u8>]) -> Vec<u8> {
    let payload: Vec<u8> = items.iter().flatten().copied().collect();
    let rec_len = (XLOG_REC_HEADER_SIZE + payload.len()) as u16;
    let mut record = build_log_record_header(1, 2, rec_len);
    record.extend_from_slice(&payload);
    record
}

fn build_inode_item(inode: u64, size: u64, links: u32) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&XLOG_ITEM_INODE.to_be_bytes());
    payload.extend_from_slice(&64u16.to_be_bytes());
    payload.extend_from_slice(&inode.to_be_bytes());
    let core = payload.len();
    payload.resize(core + 104, 0);
    payload[core..core + 2].copy_from_slice(&0x494Eu16.to_be_bytes());
    payload[core + 2..core + 4].copy_from_slice(&(0x8000u16 | 0o644).to_be_bytes());
    payload[core + 5] = 2;
    payload[core + 0x38..core + 0x40].copy_from_slice(&size.to_be_bytes());
    payload[core + 0x4C..core + 0x50].copy_from_slice(&1u32.to_be_bytes());
    payload[core + 0x60..core + 0x64].copy_from_slice(&links.to_be_bytes());
    payload
}

#[test]
fn test_parse_log_record_header() {
    let parsed = LogRecordHeader::parse(&build_log_record_header(5, 2, 256)).unwrap();
    assert_eq!(parsed.magic, XLOG_HEADER_MAGIC);
    assert_eq!(parsed.cycle, 5);
    assert_eq!(parsed.version, 2);
    assert_eq!(parsed.len, 256);
}

#[test]
fn test_log_record_header_invalid_magic() {
    let mut header = build_log_record_header(1, 2, 128);
    header[0..2].copy_from_slice(&0xABCDu16.to_be_bytes());
    let error = LogRecordHeader::parse(&header).unwrap_err();
    assert!(error.to_string().contains("magic"));
}

#[test]
fn test_collect_log_records() {
    let mut log_data = build_log_record(&[build_inode_item(42, 4096, 0)]);
    log_data.resize(4096, 0);
    let records = collect_log_records(&log_data, 4096).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].0.cycle, 1);
}

#[test]
fn test_parse_inode_item() {
    let entries = parse_log_entries(&build_inode_item(99, 8192, 0)).unwrap();
    assert!(entries
        .iter()
        .any(|entry| entry.item_type == XLOG_ITEM_INODE));
}

#[test]
fn test_recover_metadata_operations() {
    let mut log_data = build_log_record(&[build_inode_item(123, 4096, 0)]);
    log_data.resize(4096, 0);
    let entries = recover_metadata_operations(&log_data).unwrap();
    assert!(entries
        .iter()
        .any(|entry| entry.item_type == XLOG_ITEM_INODE));
}

#[test]
fn test_recover_deleted_inodes() {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&XLOG_ITEM_BUF.to_be_bytes());
    buffer.extend_from_slice(&20u16.to_be_bytes());
    buffer.extend_from_slice(b"recovered content!!");
    buffer.resize(24, 0);
    let mut log_data = build_log_record(&[build_inode_item(77, 1024, 0), buffer]);
    log_data.resize(8192, 0);
    assert!(recover_deleted_inodes(&log_data)
        .unwrap()
        .iter()
        .any(|file| file.inode == 77));
}

#[test]
fn test_extract_dirent_from_buf() {
    let mut buffer = vec![0u8; 256];
    buffer[0] = 10;
    buffer[1..11].copy_from_slice(b"readme.txt");
    buffer[11..19].copy_from_slice(&100u64.to_be_bytes());
    assert_eq!(
        extract_dirent_from_buf(&buffer),
        Some(("readme.txt".to_string(), 100))
    );
}

#[test]
fn test_extract_dirent_from_buf_no_valid_entry() {
    assert!(extract_dirent_from_buf(&vec![0u8; 256]).is_none());
}

#[test]
fn test_empty_log_data() {
    assert!(collect_log_records(&[], 4096).unwrap().is_empty());
    assert!(recover_metadata_operations(&[]).unwrap().is_empty());
    assert!(recover_deleted_inodes(&[]).unwrap().is_empty());
}

#[test]
fn test_record_len_when_zero() {
    let parsed = LogRecordHeader::parse(&build_log_record_header(1, 2, 0)).unwrap();
    assert_eq!(parsed.record_len(4096), 4096);
}

#[test]
fn test_parse_logged_inode_core_nlink_nonzero() {
    let payload = build_inode_item(88, 512, 1);
    let data = &payload[4..];
    let core = parse_logged_inode_core(data).unwrap();
    assert!(!core.is_deleted);
}
