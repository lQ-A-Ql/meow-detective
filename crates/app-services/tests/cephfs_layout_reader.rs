use std::collections::BTreeMap;

use app_services::ceph_reconstruction::{
    CephFsDataRangeReader, CephFsFileDataDescriptor, CephFsFileDataReadError, CephFsObjectLocator,
    CephFsObjectMetadata, CephFsObjectRange, CephFsObjectRangeReader, CephFsObjectReadError,
    CephFsObjectReadProvenance, CEPHFS_DATA_LOCATOR_VERSION, MAX_CEPHFS_OBJECT_RANGE_LENGTH,
};
use ceph_wire::CephFsFileLayout;

const KIB: u32 = 1024;
const FILESYSTEM_IDENTITY: &str = "ceph-fs:cluster-a:1:17:7";

struct FixtureObjectReader {
    objects: BTreeMap<String, Vec<u8>>,
    mismatched_response: bool,
}

impl CephFsObjectRangeReader for FixtureObjectReader {
    fn inspect_object(
        &mut self,
        locator: &CephFsObjectLocator,
    ) -> Result<CephFsObjectMetadata, CephFsObjectReadError> {
        let bytes = self.objects.get(&locator.canonical()).ok_or_else(|| {
            CephFsObjectReadError::ObjectNotFound {
                locator: locator.canonical(),
            }
        })?;
        Ok(CephFsObjectMetadata {
            filesystem_identity: FILESYSTEM_IDENTITY.to_string(),
            locator: locator.canonical(),
            object_size: bytes.len() as u64,
            provenance: provenance(),
        })
    }

    fn read_range(
        &mut self,
        locator: &CephFsObjectLocator,
        offset: u64,
        length: usize,
    ) -> Result<CephFsObjectRange, CephFsObjectReadError> {
        let bytes = self.objects.get(&locator.canonical()).ok_or_else(|| {
            CephFsObjectReadError::ObjectNotFound {
                locator: locator.canonical(),
            }
        })?;
        let start = offset as usize;
        let end = start + length;
        if end > bytes.len() {
            return Err(CephFsObjectReadError::RangeOutOfBounds {
                locator: locator.canonical(),
                object_size: bytes.len() as u64,
            });
        }
        Ok(CephFsObjectRange {
            filesystem_identity: if self.mismatched_response {
                "wrong-filesystem".to_string()
            } else {
                FILESYSTEM_IDENTITY.to_string()
            },
            locator: locator.canonical(),
            object_size: bytes.len() as u64,
            offset,
            bytes: bytes[start..end].to_vec(),
            provenance: provenance(),
        })
    }
}

#[test]
fn reads_multi_object_stripes_and_emits_complete_cache_identity() {
    let descriptor = object_descriptor(320 * KIB as u64);
    let objects = BTreeMap::from([
        (locator(0).canonical(), vec![b'A'; 128 * KIB as usize]),
        (locator(1).canonical(), vec![b'B'; 128 * KIB as usize]),
        (locator(2).canonical(), vec![b'C'; 64 * KIB as usize]),
    ]);
    let mut reader = CephFsDataRangeReader::new(
        descriptor,
        FixtureObjectReader {
            objects,
            mismatched_response: false,
        },
    )
    .unwrap();

    let range = reader.read_range(0, 320 * KIB as usize).unwrap();

    assert_eq!(range.bytes.len(), 320 * KIB as usize);
    assert_eq!(
        &range.bytes[0..64 * KIB as usize],
        vec![b'A'; 64 * KIB as usize]
    );
    assert_eq!(range.bytes[64 * KIB as usize], b'B');
    assert_eq!(range.bytes[128 * KIB as usize], b'A');
    assert_eq!(range.bytes[192 * KIB as usize], b'B');
    assert_eq!(range.bytes[256 * KIB as usize], b'C');
    assert_eq!(range.object_reads.len(), 5);
    let key = &range.object_reads[0].cache_key;
    assert_eq!(key.filesystem_identity, FILESYSTEM_IDENTITY);
    assert_eq!(key.pool_id, 8);
    assert_eq!(key.object_name, "1.00000000");
    assert_eq!(key.fsmap_epoch, 17);
    assert_eq!(key.locator_version, CEPHFS_DATA_LOCATOR_VERSION);
}

#[test]
fn inline_data_is_bounded_without_touching_the_object_reader() {
    let descriptor = CephFsFileDataDescriptor::new(
        FILESYSTEM_IDENTITY,
        1,
        17,
        1,
        6,
        CephFsFileLayout::new(0, 0, 0, -1, "").unwrap(),
        Some(b"abcdef".to_vec()),
    )
    .unwrap();
    let mut reader = CephFsDataRangeReader::new(
        descriptor,
        FixtureObjectReader {
            objects: BTreeMap::new(),
            mismatched_response: false,
        },
    )
    .unwrap();

    let range = reader.read_range(2, 3).unwrap();

    assert_eq!(range.bytes, b"cde");
    assert!(range.object_reads.is_empty());
}

#[test]
fn missing_objects_bad_responses_and_invalid_ranges_fail_closed() {
    let descriptor = object_descriptor((2 * 1024 * 1024) as u64);
    let mut missing = CephFsDataRangeReader::new(
        descriptor.clone(),
        FixtureObjectReader {
            objects: BTreeMap::new(),
            mismatched_response: false,
        },
    )
    .unwrap();
    assert!(matches!(
        missing.read_range(0, 1),
        Err(CephFsFileDataReadError::Object(
            CephFsObjectReadError::ObjectNotFound { .. }
        ))
    ));
    assert!(matches!(
        missing.read_range(0, MAX_CEPHFS_OBJECT_RANGE_LENGTH + 1),
        Err(CephFsFileDataReadError::RangeTooLarge { .. })
    ));
    assert!(matches!(
        missing.read_range(u64::MAX, 1),
        Err(CephFsFileDataReadError::RangeOverflow)
    ));

    let mut mismatched = CephFsDataRangeReader::new(
        descriptor,
        FixtureObjectReader {
            objects: BTreeMap::from([(locator(0).canonical(), vec![0; 128 * KIB as usize])]),
            mismatched_response: true,
        },
    )
    .unwrap();
    assert!(matches!(
        mismatched.read_range(0, 1),
        Err(CephFsFileDataReadError::ResponseMismatch { .. })
    ));
}

fn object_descriptor(file_size: u64) -> CephFsFileDataDescriptor {
    CephFsFileDataDescriptor::new(
        FILESYSTEM_IDENTITY,
        1,
        17,
        1,
        file_size,
        CephFsFileLayout::new(64 * KIB, 2, 128 * KIB, 8, "").unwrap(),
        None,
    )
    .unwrap()
}

fn locator(object_number: u32) -> CephFsObjectLocator {
    CephFsObjectLocator::new(
        1,
        8,
        Vec::new(),
        format!("1.{object_number:08x}").into_bytes(),
        17,
    )
    .unwrap()
}

fn provenance() -> Vec<CephFsObjectReadProvenance> {
    vec![CephFsObjectReadProvenance {
        data_source_id: "source-a".to_string(),
        inventory_id: "inventory-a".to_string(),
        object_identity_sha256: "a".repeat(64),
    }]
}
