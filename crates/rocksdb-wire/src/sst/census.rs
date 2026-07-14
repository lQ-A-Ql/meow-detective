use std::collections::{BTreeMap, HashSet};
use std::fmt::{Debug, Formatter};

use crate::{Result, RocksDbWireError};

use super::{KeySpaceBucket, KeySpaceCensus, SstReadOptions, KEY_SPACE_SUMMARY_VERSION};

const MAX_COLUMN_FAMILY_NAME_BYTES: usize = 1024;
const MAX_BUCKET_NAME_BYTES: usize = 128;
const MAX_PREFIX_BYTES: usize = 1024;
const MAX_PREFIX_RULES: usize = 256;

#[derive(Clone, PartialEq, Eq)]
pub struct KeySpacePrefixRule {
    bucket_name: String,
    prefix: Vec<u8>,
}

impl KeySpacePrefixRule {
    pub fn new(bucket_name: impl Into<String>, prefix: impl Into<Vec<u8>>) -> Result<Self> {
        let bucket_name = bucket_name.into();
        let prefix = prefix.into();
        validate_bucket_name(&bucket_name)?;
        if prefix.is_empty() {
            return Err(invalid_context("prefix rules must not use an empty prefix"));
        }
        if prefix.len() > MAX_PREFIX_BYTES {
            return Err(invalid_context("prefix rule exceeds the byte limit"));
        }
        Ok(Self {
            bucket_name,
            prefix,
        })
    }
}

impl Debug for KeySpacePrefixRule {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeySpacePrefixRule")
            .field("bucket_name", &self.bucket_name)
            .field("prefix_length", &self.prefix.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct KeySpaceCensusContext {
    expected_column_family_name: String,
    unknown_bucket_name: String,
    classifier: KeyClassifier,
}

impl KeySpaceCensusContext {
    pub fn unclassified(
        expected_column_family_name: impl Into<String>,
        unknown_bucket_name: impl Into<String>,
    ) -> Result<Self> {
        Self::build(
            expected_column_family_name.into(),
            unknown_bucket_name.into(),
            KeyClassifier::PrefixRules(Vec::new()),
        )
    }

    pub fn single_bucket(
        expected_column_family_name: impl Into<String>,
        bucket_name: impl Into<String>,
        unknown_bucket_name: impl Into<String>,
    ) -> Result<Self> {
        let bucket_name = bucket_name.into();
        validate_bucket_name(&bucket_name)?;
        Self::build(
            expected_column_family_name.into(),
            unknown_bucket_name.into(),
            KeyClassifier::SingleBucket(bucket_name),
        )
    }

    pub fn prefix_buckets(
        expected_column_family_name: impl Into<String>,
        unknown_bucket_name: impl Into<String>,
        rules: Vec<KeySpacePrefixRule>,
    ) -> Result<Self> {
        if rules.len() > MAX_PREFIX_RULES {
            return Err(invalid_context("prefix rule count exceeds the limit"));
        }
        let mut prefixes = HashSet::with_capacity(rules.len());
        for rule in &rules {
            if !prefixes.insert(rule.prefix.as_slice()) {
                return Err(invalid_context("prefix rules contain a duplicate prefix"));
            }
        }
        Self::build(
            expected_column_family_name.into(),
            unknown_bucket_name.into(),
            KeyClassifier::PrefixRules(rules),
        )
    }

    fn build(
        expected_column_family_name: String,
        unknown_bucket_name: String,
        classifier: KeyClassifier,
    ) -> Result<Self> {
        validate_column_family_name(&expected_column_family_name)?;
        validate_bucket_name(&unknown_bucket_name)?;
        if classifier
            .bucket_names()
            .any(|bucket| bucket == unknown_bucket_name)
        {
            return Err(invalid_context(
                "classified and unknown bucket names must differ",
            ));
        }
        Ok(Self {
            expected_column_family_name,
            unknown_bucket_name,
            classifier,
        })
    }

    pub(crate) fn validate_column_family(&self, actual: &str) -> Result<()> {
        if actual != self.expected_column_family_name {
            return Err(RocksDbWireError::SstCensusColumnFamilyMismatch);
        }
        Ok(())
    }

    fn classify(&self, user_key: &[u8]) -> &str {
        if user_key.is_empty() {
            return &self.unknown_bucket_name;
        }
        match &self.classifier {
            KeyClassifier::SingleBucket(bucket_name) => bucket_name,
            KeyClassifier::PrefixRules(rules) => rules
                .iter()
                .find(|rule| user_key.starts_with(&rule.prefix))
                .map_or(&self.unknown_bucket_name, |rule| &rule.bucket_name),
        }
    }
}

impl Debug for KeySpaceCensusContext {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeySpaceCensusContext")
            .field(
                "expected_column_family_name_length",
                &self.expected_column_family_name.len(),
            )
            .field("unknown_bucket_name", &self.unknown_bucket_name)
            .field("classifier", &self.classifier)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
enum KeyClassifier {
    SingleBucket(String),
    PrefixRules(Vec<KeySpacePrefixRule>),
}

impl KeyClassifier {
    fn bucket_names(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Self::SingleBucket(bucket_name) => Box::new(std::iter::once(bucket_name.as_str())),
            Self::PrefixRules(rules) => {
                Box::new(rules.iter().map(|rule| rule.bucket_name.as_str()))
            }
        }
    }
}

impl Debug for KeyClassifier {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SingleBucket(bucket_name) => formatter
                .debug_struct("SingleBucket")
                .field("bucket_name", bucket_name)
                .finish(),
            Self::PrefixRules(rules) => formatter
                .debug_struct("PrefixRules")
                .field("rules", rules)
                .finish(),
        }
    }
}

pub(crate) struct CensusBuilder<'a> {
    context: &'a KeySpaceCensusContext,
    options: SstReadOptions,
    scanned_entries: u64,
    scanned_decompressed_bytes: u64,
    buckets: BTreeMap<&'a str, BucketAccumulator>,
}

impl<'a> CensusBuilder<'a> {
    pub(crate) fn new(options: SstReadOptions, context: &'a KeySpaceCensusContext) -> Self {
        Self {
            context,
            options,
            scanned_entries: 0,
            scanned_decompressed_bytes: 0,
            buckets: BTreeMap::new(),
        }
    }

    pub(crate) fn add_decompressed_bytes(&mut self, bytes: u64) -> Result<()> {
        let total = self.scanned_decompressed_bytes.checked_add(bytes).ok_or(
            RocksDbWireError::LengthOverflow {
                context: "SST census decompressed bytes",
            },
        )?;
        if total > self.options.max_census_decompressed_bytes {
            return Err(RocksDbWireError::SstCensusDecompressedLimit {
                limit: self.options.max_census_decompressed_bytes,
            });
        }
        self.scanned_decompressed_bytes = total;
        Ok(())
    }

    pub(crate) fn observe(&mut self, user_key: &[u8]) -> Result<()> {
        if self.scanned_entries >= self.options.max_census_entries {
            return Err(RocksDbWireError::SstCensusEntryLimit {
                limit: self.options.max_census_entries,
            });
        }
        self.scanned_entries =
            self.scanned_entries
                .checked_add(1)
                .ok_or(RocksDbWireError::LengthOverflow {
                    context: "SST census entry count",
                })?;
        let name = self.context.classify(user_key);
        self.buckets
            .entry(name)
            .or_default()
            .observe(user_key.len())
    }

    pub(crate) fn finish(self) -> KeySpaceCensus {
        let buckets = self
            .buckets
            .into_iter()
            .map(|(name, bucket)| KeySpaceBucket {
                name: name.to_owned(),
                entries: bucket.entries,
                min_user_key_length: bucket.min_length,
                max_user_key_length: bucket.max_length,
            })
            .collect();
        KeySpaceCensus {
            version: KEY_SPACE_SUMMARY_VERSION,
            scanned_entries: self.scanned_entries,
            scanned_decompressed_bytes: self.scanned_decompressed_bytes,
            complete: true,
            buckets,
        }
    }
}

#[derive(Default)]
struct BucketAccumulator {
    entries: u64,
    min_length: u32,
    max_length: u32,
}

impl BucketAccumulator {
    fn observe(&mut self, length: usize) -> Result<()> {
        let length = u32::try_from(length).map_err(|_| RocksDbWireError::LengthOverflow {
            context: "SST census key length",
        })?;
        self.entries = self
            .entries
            .checked_add(1)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "SST census bucket entry count",
            })?;
        if self.entries == 1 {
            self.min_length = length;
        } else {
            self.min_length = self.min_length.min(length);
        }
        self.max_length = self.max_length.max(length);
        Ok(())
    }
}

fn validate_column_family_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > MAX_COLUMN_FAMILY_NAME_BYTES || name.contains('\0') {
        return Err(invalid_context(
            "expected column family name is empty, oversized, or contains NUL",
        ));
    }
    Ok(())
}

fn validate_bucket_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= MAX_BUCKET_NAME_BYTES
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'));
    if !valid {
        return Err(invalid_context(
            "bucket name is not a sanitized ASCII label",
        ));
    }
    Ok(())
}

fn invalid_context(reason: &'static str) -> RocksDbWireError {
    RocksDbWireError::InvalidSstCensusContext { reason }
}
