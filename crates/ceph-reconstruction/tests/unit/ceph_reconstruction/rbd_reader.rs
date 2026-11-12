use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use ceph_wire::RbdHeadImageLayout;

use super::*;

const OBJECT_SIZE: usize = 4096;
const PREFIX: &str = "rbd_data.image";

#[derive(Default)]
struct MemoryProvider {
    objects: BTreeMap<String, Vec<u8>>,
    requests: Arc<Mutex<Vec<RbdObjectReadRequest>>>,
    fail_identity: Option<String>,
    returned_identity: Option<String>,
    short_read: bool,
}

impl MemoryProvider {
    fn with_object(mut self, object_no: u64, bytes: Vec<u8>) -> Self {
        self.objects.insert(object_name(object_no), bytes);
        self
    }
}

impl RbdObjectProvider for MemoryProvider {
    fn read_object_range(
        &mut self,
        request: &RbdObjectReadRequest,
        output: &mut [u8],
    ) -> Result<RbdObjectReadOutcome, RbdObjectProviderError> {
        self.requests.lock().unwrap().push(request.clone());
        if self.fail_identity.as_deref() == Some(request.object_identity.as_str()) {
            return Err(RbdObjectProviderError::ReadFailed {
                object_identity: request.object_identity.clone(),
                reason: "fixture failure".to_string(),
            });
        }
        let Some(bytes) = self.objects.get(&request.object_identity) else {
            return Ok(RbdObjectReadOutcome::Missing);
        };
        let start = usize::try_from(request.object_offset).unwrap();
        let end = start + request.length;
        output.copy_from_slice(&bytes[start..end]);
        Ok(RbdObjectReadOutcome::Present {
            object_identity: self
                .returned_identity
                .clone()
                .unwrap_or_else(|| request.object_identity.clone()),
            bytes_read: request.length - usize::from(self.short_read),
        })
    }
}

#[test]
fn reads_only_planned_ranges_across_object_boundaries() {
    let mut first = vec![0u8; OBJECT_SIZE];
    first[OBJECT_SIZE - 4..].copy_from_slice(b"ABCD");
    let mut second = vec![0u8; OBJECT_SIZE];
    second[..4].copy_from_slice(b"EFGH");
    let provider = MemoryProvider::default()
        .with_object(0, first)
        .with_object(1, second);
    let requests = Arc::clone(&provider.requests);
    let mut reader = reader(provider, (OBJECT_SIZE * 2) as u64, 0, safe_context()).unwrap();

    reader
        .seek(SeekFrom::Start((OBJECT_SIZE - 4) as u64))
        .unwrap();
    let mut output = [0u8; 8];
    reader.read_exact(&mut output).unwrap();

    assert_eq!(&output, b"ABCDEFGH");
    assert_eq!(
        requests.lock().unwrap().as_slice(),
        &[
            RbdObjectReadRequest {
                object_no: 0,
                object_identity: object_name(0),
                object_offset: (OBJECT_SIZE - 4) as u64,
                length: 4,
            },
            RbdObjectReadRequest {
                object_no: 1,
                object_identity: object_name(1),
                object_offset: 0,
                length: 4,
            },
        ]
    );
}

#[test]
fn zero_fills_a_missing_object_within_the_image() {
    let provider = MemoryProvider::default();
    let requests = Arc::clone(&provider.requests);
    let mut reader = reader(provider, (OBJECT_SIZE * 2) as u64, 0, safe_context()).unwrap();
    reader.seek(SeekFrom::Start(OBJECT_SIZE as u64)).unwrap();

    let mut output = [0xff; 32];
    reader.read_exact(&mut output).unwrap();

    assert_eq!(output, [0; 32]);
    assert_eq!(requests.lock().unwrap()[0].length, 32);
}

#[test]
fn clips_reads_at_eof_and_rejects_out_of_bounds_seeks() {
    let provider = MemoryProvider::default().with_object(0, vec![0x5a; OBJECT_SIZE]);
    let mut reader = reader(provider, 64, 0, safe_context()).unwrap();
    reader.seek(SeekFrom::Start(60)).unwrap();

    let mut output = [0xcc; 8];
    assert_eq!(reader.read(&mut output).unwrap(), 4);
    assert_eq!(&output[..4], &[0x5a; 4]);
    assert_eq!(&output[4..], &[0xcc; 4]);
    assert_eq!(reader.read(&mut output).unwrap(), 0);
    assert_eq!(
        reader.seek(SeekFrom::End(1)).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert!(matches!(
        reader.read_at(65, &mut output),
        Err(RbdReadError::RangeOutOfBounds {
            offset: 65,
            image_size: 64
        })
    ));
}

#[test]
fn rejects_features_that_change_missing_object_or_pool_semantics() {
    for unsupported in [RBD_FEATURE_DATA_POOL, RBD_FEATURE_DIRTY_CACHE, 1u64 << 20] {
        assert!(matches!(
            reader(MemoryProvider::default(), 64, unsupported, safe_context()),
            Err(RbdReadError::UnsupportedFeatures { .. })
                | Err(RbdReadError::ParentCloneUnsupported)
        ));
    }
}

#[test]
fn rejects_journaling_even_when_the_data_layout_is_available() {
    assert!(matches!(
        reader(
            MemoryProvider::default(),
            64,
            RBD_FEATURE_JOURNALING,
            safe_context()
        ),
        Err(RbdReadError::JournalingUnsupported)
    ));
}

#[test]
fn rejects_parent_snapshot_and_encryption_contexts() {
    assert!(matches!(
        reader(
            MemoryProvider::default(),
            64,
            0,
            RbdReadContext {
                has_parent: true,
                ..safe_context()
            }
        ),
        Err(RbdReadError::ParentCloneUnsupported)
    ));
    assert!(matches!(
        reader(
            MemoryProvider::default(),
            64,
            0,
            RbdReadContext {
                snapshot_id: Some(7),
                ..safe_context()
            }
        ),
        Err(RbdReadError::SnapshotUnsupported)
    ));
    assert!(matches!(
        reader(
            MemoryProvider::default(),
            64,
            0,
            RbdReadContext {
                encrypted: true,
                ..safe_context()
            }
        ),
        Err(RbdReadError::EncryptionUnsupported)
    ));
}

#[test]
fn rejects_parent_clone_operation_features() {
    assert!(matches!(
        reader(
            MemoryProvider::default(),
            64,
            0,
            RbdReadContext {
                operation_features: RBD_OPERATION_FEATURE_CLONE_PARENT,
                ..safe_context()
            }
        ),
        Err(RbdReadError::ParentCloneUnsupported)
    ));
}

#[test]
fn accepts_only_explicitly_read_safe_features() {
    let provider = MemoryProvider::default().with_object(0, vec![0x33; OBJECT_SIZE]);
    let read_safe_features = RBD_FEATURE_STRIPINGV2
        | RBD_FEATURE_LAYERING
        | RBD_FEATURE_EXCLUSIVE_LOCK
        | RBD_FEATURE_OBJECT_MAP
        | RBD_FEATURE_FAST_DIFF
        | RBD_FEATURE_DEEP_FLATTEN
        | RBD_FEATURE_OPERATIONS
        | RBD_FEATURE_NON_PRIMARY;
    let mut reader = reader(provider, 64, read_safe_features, safe_context()).unwrap();
    let mut output = [0u8; 8];

    reader.read_exact(&mut output).unwrap();

    assert_eq!(output, [0x33; 8]);
}

#[test]
fn rejects_migration_state_that_can_redirect_image_reads() {
    assert!(matches!(
        reader(
            MemoryProvider::default(),
            64,
            RBD_FEATURE_MIGRATING,
            safe_context()
        ),
        Err(RbdReadError::UnsupportedFeatures { unsupported, .. })
            if unsupported == RBD_FEATURE_MIGRATING
    ));
}

#[test]
fn preserves_typed_provider_failures() {
    let provider = MemoryProvider {
        fail_identity: Some(object_name(0)),
        ..MemoryProvider::default()
    };
    let mut reader = reader(provider, 64, 0, safe_context()).unwrap();

    assert!(matches!(
        reader.read_at(0, &mut [0u8; 8]),
        Err(RbdReadError::Provider(
            RbdObjectProviderError::ReadFailed { .. }
        ))
    ));
}

#[test]
fn rejects_provider_object_identity_mismatch() {
    let provider = MemoryProvider {
        returned_identity: Some(object_name(1)),
        ..MemoryProvider::default().with_object(0, vec![0x22; OBJECT_SIZE])
    };
    let mut reader = reader(provider, 64, 0, safe_context()).unwrap();

    assert!(matches!(
        reader.read_at(0, &mut [0u8; 8]),
        Err(RbdReadError::ObjectIdentityMismatch { .. })
    ));
}

#[test]
fn rejects_provider_short_reads() {
    let provider = MemoryProvider {
        short_read: true,
        ..MemoryProvider::default().with_object(0, vec![0x22; OBJECT_SIZE])
    };
    let mut reader = reader(provider, 64, 0, safe_context()).unwrap();

    assert!(matches!(
        reader.read_at(0, &mut [0u8; 8]),
        Err(RbdReadError::ShortObjectRead {
            expected: 8,
            actual: 7,
            ..
        })
    ));
}

fn reader(
    provider: MemoryProvider,
    image_size: u64,
    features: u64,
    context: RbdReadContext,
) -> Result<RbdEvidenceReader, RbdReadError> {
    let layout =
        RbdHeadImageLayout::new_with_features(image_size, 12, features, PREFIX, 0, 0).unwrap();
    RbdEvidenceReader::new(Box::new(provider), layout, context)
}

fn safe_context() -> RbdReadContext {
    RbdReadContext {
        operation_features: 0,
        has_parent: false,
        snapshot_id: None,
        encrypted: false,
    }
}

fn object_name(object_no: u64) -> String {
    format!("{PREFIX}.{object_no:016x}")
}
