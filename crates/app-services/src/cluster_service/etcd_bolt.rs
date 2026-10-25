use super::kubernetes_parser_error::Result;
use crate::bolt_reader_service::{decode, BoltReadMode};
use std::{path::Path, sync::atomic::AtomicBool};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtcdBoltSummary {
    pub page_size: u32,
    pub txid: u64,
    pub root_page: Option<u64>,
    pub bucket_count: usize,
    pub key_count: usize,
    pub entries: Vec<EtcdBoltEntry>,
    pub checksum_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtcdBoltEntry {
    pub bucket_path: String,
    pub key: String,
    pub value_length: Option<u64>,
    pub nested_bucket: bool,
    pub value_redacted: bool,
}

pub fn parse_etcd_bolt_metadata(
    workspace: &Path,
    bytes: &[u8],
    cancel: &AtomicBool,
) -> Result<EtcdBoltSummary> {
    let read = decode(workspace, bytes, BoltReadMode::EtcdMetadata, cancel)?;
    let report = read.report();
    let entries: Vec<_> = report
        .entries
        .iter()
        .map(|entry| EtcdBoltEntry {
            bucket_path: entry
                .bucket_path
                .iter()
                .map(|part| display_key(part))
                .collect::<Vec<_>>()
                .join("/"),
            key: display_key(&entry.key),
            value_length: entry.value_length,
            nested_bucket: entry.is_bucket,
            value_redacted: entry.value_length.is_some_and(|length| length > 0),
        })
        .collect();
    Ok(EtcdBoltSummary {
        page_size: report.page_size,
        txid: report.txid,
        root_page: None,
        bucket_count: report.visited_buckets as usize,
        key_count: entries.iter().filter(|entry| !entry.nested_bucket).count(),
        entries,
        checksum_verified: report.metadata_checksum_verified,
    })
}

fn display_key(bytes: &[u8]) -> String {
    if bytes
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        format!("hex:{}", hex::encode(bytes))
    }
}
