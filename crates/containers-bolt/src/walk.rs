use crate::{BoltEntry, BoltError, BoltMetadata, BoltReadOptions, BoltReadReport};
use bbolt_rs::{BucketApi, BucketImpl, CursorApi};

pub(crate) struct Walker<'a> {
    options: &'a BoltReadOptions,
    output_bytes: usize,
    pub(crate) report: BoltReadReport,
}

impl<'a> Walker<'a> {
    pub(crate) fn new(options: &'a BoltReadOptions, metadata: BoltMetadata) -> Self {
        Self {
            options,
            output_bytes: 0,
            report: BoltReadReport {
                metadata,
                entries: Vec::new(),
                visited_entries: 0,
                visited_buckets: 0,
                missing_bucket: None,
            },
        }
    }

    pub(crate) fn record(
        &mut self,
        path: &[Vec<u8>],
        key: &[u8],
        value: Option<&[u8]>,
        is_bucket: bool,
    ) -> Result<(), BoltError> {
        if self.report.visited_entries >= self.options.max_entries as u64 {
            return Err(BoltError::EntryLimit);
        }
        self.report.visited_entries += 1;
        if is_bucket {
            self.report.visited_buckets += 1
        }
        if (is_bucket && !self.options.include_buckets)
            || (!is_bucket
                && !self.options.record_keys.is_empty()
                && !self
                    .options
                    .record_keys
                    .iter()
                    .any(|candidate| candidate == key))
            || (!is_bucket && !path.ends_with(&self.options.parent_suffix))
            || (if is_bucket {
                self.options.bucket_parent_depth
            } else {
                self.options.parent_depth
            })
            .is_some_and(|depth| path.len() != self.options.prefix.len() + depth)
        {
            return Ok(());
        }
        let include_value = self
            .options
            .value_keys
            .iter()
            .any(|candidate| candidate == key);
        let raw_length = if include_value {
            value.map_or(0, |value| value.len())
        } else {
            0
        };
        let length = path
            .iter()
            .try_fold(key.len().saturating_add(raw_length), |sum, part| {
                sum.checked_add(part.len())
            })
            .ok_or(BoltError::OutputLimit)?;
        self.output_bytes = self
            .output_bytes
            .checked_add(length)
            .ok_or(BoltError::OutputLimit)?;
        if self.output_bytes > self.options.max_output_bytes {
            return Err(BoltError::OutputLimit);
        }
        self.report.entries.push(BoltEntry {
            bucket_path: path.to_vec(),
            key: key.to_vec(),
            value_length: value.map(|value| value.len() as u64),
            value: if include_value {
                value.map(<[u8]>::to_vec)
            } else {
                None
            },
            is_bucket,
        });
        Ok(())
    }

    pub(crate) fn bucket<'tx>(
        &mut self,
        bucket: BucketImpl<'tx, '_>,
        path: &mut Vec<Vec<u8>>,
    ) -> Result<(), BoltError> {
        if path.len() > self.options.max_depth {
            return Err(BoltError::DepthLimit);
        }
        let mut cursor = bucket.cursor();
        let mut item = cursor.first();
        while let Some((key, value)) = item {
            self.record(path, key, value, value.is_none())?;
            if value.is_none() {
                let key = key.to_vec();
                let child = bucket.bucket(&key).ok_or(BoltError::InvalidBucket)?;
                path.push(key);
                self.bucket(child, path)?;
                path.pop();
            }
            item = cursor.next();
        }
        Ok(())
    }
}
