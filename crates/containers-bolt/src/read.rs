use bbolt_rs::{Bolt, DbApi, TxApi};
use std::path::Path;

use crate::{walk::Walker, BoltError, BoltMetadata, BoltReadOptions, BoltReadReport};

pub fn read_bolt(path: &Path, options: &BoltReadOptions) -> Result<BoltReadReport, BoltError> {
    if options.max_entries == 0
        || options.max_output_bytes == 0
        || options.max_depth == 0
        || options.max_depth > 64
        || options.prefix.len() > options.max_depth
    {
        return Err(BoltError::InvalidOptions);
    }
    let file = std::fs::metadata(path)?;
    if !file.is_file() || file.len() > options.max_file_bytes {
        return Err(BoltError::FileLimit);
    }
    let db = Bolt::open_ro(path)?;
    let tx = db.begin()?;
    let metadata = BoltMetadata {
        page_size: u32::try_from(db.info().page_size).map_err(|_| BoltError::InvalidOptions)?,
        txid: tx.id().0,
        metadata_checksum_verified: true,
    };
    let mut walker = Walker::new(options, metadata);
    if options.prefix.is_empty() {
        for (name, bucket) in tx.iter_buckets() {
            walker.record(&[], name, None, true)?;
            let mut path = vec![name.to_vec()];
            walker.bucket(bucket, &mut path)?;
        }
    } else {
        for depth in 1..=options.prefix.len() {
            if tx.bucket_path(&options.prefix[..depth]).is_none() {
                walker.report.missing_bucket = Some(depth - 1);
                return Ok(walker.report);
            }
        }
        let bucket = tx
            .bucket_path(&options.prefix)
            .ok_or(BoltError::InvalidBucket)?;
        walker.bucket(bucket, &mut options.prefix.clone())?;
    }
    Ok(walker.report)
}
