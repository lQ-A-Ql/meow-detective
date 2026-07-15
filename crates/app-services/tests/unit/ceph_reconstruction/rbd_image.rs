use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use ceph_wire::RbdImageMetadata;

use super::*;
use crate::ceph_reconstruction::{
    RbdObjectProviderError, RbdObjectReadOutcome, RbdObjectReadRequest, RbdReadContext,
    RBD_FEATURE_LAYERING,
};
use crate::datasource_service::{ImageFilesystemKind, ImageFilesystemSource, PartitionStatus};

const OBJECT_SIZE: usize = 4096;
const IMAGE_SIZE: usize = OBJECT_SIZE * 32;
const OBJECT_PREFIX: &str = "rbd_data.image-test";

#[derive(Default)]
struct MemoryProvider {
    objects: BTreeMap<String, Vec<u8>>,
    requests: Arc<Mutex<Vec<RbdObjectReadRequest>>>,
}

impl MemoryProvider {
    fn from_image(image: &[u8]) -> Self {
        let objects = image
            .chunks(OBJECT_SIZE)
            .enumerate()
            .map(|(object_no, bytes)| (object_name(object_no as u64), bytes.to_vec()))
            .collect();
        Self {
            objects,
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl RbdObjectProvider for MemoryProvider {
    fn read_object_range(
        &mut self,
        request: &RbdObjectReadRequest,
        output: &mut [u8],
    ) -> Result<RbdObjectReadOutcome, RbdObjectProviderError> {
        self.requests.lock().unwrap().push(request.clone());
        let Some(bytes) = self.objects.get(&request.object_identity) else {
            return Ok(RbdObjectReadOutcome::Missing);
        };
        let start = usize::try_from(request.object_offset).unwrap();
        let end = start + request.length;
        output.copy_from_slice(&bytes[start..end]);
        Ok(RbdObjectReadOutcome::Present {
            object_identity: request.object_identity.clone(),
            bytes_read: request.length,
        })
    }
}

#[test]
fn builds_layout_and_reader_from_descriptor_without_a_host_path() {
    let image = patterned_image();
    let provider = MemoryProvider::from_image(&image);
    let requests = Arc::clone(&provider.requests);
    let descriptor = descriptor(0);

    let layout = build_rbd_head_layout(&descriptor).expect("build RBD layout");
    assert_eq!(layout.image_size, IMAGE_SIZE as u64);
    assert_eq!(layout.object_size, OBJECT_SIZE as u64);
    assert_eq!(layout.object_prefix, OBJECT_PREFIX);

    let mut reader =
        open_rbd_head_image(&descriptor, Box::new(provider)).expect("open RBD head image");
    reader
        .seek(SeekFrom::Start((OBJECT_SIZE - 4) as u64))
        .unwrap();
    let mut output = [0u8; 8];
    reader.read_exact(&mut output).unwrap();

    assert_eq!(&output, &image[OBJECT_SIZE - 4..OBJECT_SIZE + 4]);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].object_identity, object_name(0));
    assert_eq!(requests[1].object_identity, object_name(1));
}

#[test]
fn detects_an_mbr_ext4_partition_across_rbd_objects() {
    let image = mbr_ext4_image();
    let provider = MemoryProvider::from_image(&image);
    let requests = Arc::clone(&provider.requests);

    let probe = detect_rbd_image_filesystem(&descriptor(0), Box::new(provider))
        .expect("detect synthetic RBD filesystem");

    assert_eq!(probe.candidates.len(), 1);
    assert_eq!(probe.candidates[0].kind, ImageFilesystemKind::Ext4);
    assert_eq!(
        probe.candidates[0].source,
        ImageFilesystemSource::MbrPartition
    );
    assert_eq!(probe.candidates[0].offset, OBJECT_SIZE as u64);
    assert_eq!(probe.partitions.len(), 1);
    assert_eq!(probe.partitions[0].status, PartitionStatus::Supported);

    let requests = requests.lock().unwrap();
    assert!(requests
        .iter()
        .any(|request| request.object_identity == object_name(0)));
    assert!(requests
        .iter()
        .any(|request| request.object_identity == object_name(1)));
}

#[test]
fn rejects_unsupported_head_features_before_reading_objects() {
    let provider = MemoryProvider::from_image(&patterned_image());
    let requests = Arc::clone(&provider.requests);

    let error = detect_rbd_image_filesystem(&descriptor(RBD_FEATURE_LAYERING), Box::new(provider))
        .unwrap_err();

    assert!(matches!(
        error,
        RbdImageError::Open {
            source: RbdReadError::ParentCloneUnsupported,
            ..
        }
    ));
    assert!(requests.lock().unwrap().is_empty());
}

fn descriptor(features: u64) -> RbdImageDescriptor {
    RbdImageDescriptor {
        metadata: RbdImageMetadata {
            name: "vm-100-disk-0".to_string(),
            id: "image-test".to_string(),
            object_prefix: OBJECT_PREFIX.to_string(),
            image_size: IMAGE_SIZE as u64,
            order: 12,
            features,
            stripe_unit: 0,
            stripe_count: 0,
            data_pool_id: 8,
        },
        scope_identity: "perPool:i8:-:0000000000000010".to_string(),
        context: RbdReadContext {
            operation_features: 0,
            has_parent: false,
            snapshot_id: None,
            encrypted: false,
        },
    }
}

fn patterned_image() -> Vec<u8> {
    (0..IMAGE_SIZE).map(|index| index as u8).collect()
}

fn mbr_ext4_image() -> Vec<u8> {
    let mut image = vec![0u8; IMAGE_SIZE];
    let partition_entry = &mut image[446..462];
    partition_entry[4] = 0x83;
    partition_entry[8..12].copy_from_slice(&8u32.to_le_bytes());
    partition_entry[12..16].copy_from_slice(&128u32.to_le_bytes());
    image[510..512].copy_from_slice(&[0x55, 0xaa]);

    let ext4_magic_offset = OBJECT_SIZE + 1024 + 0x38;
    image[ext4_magic_offset..ext4_magic_offset + 2].copy_from_slice(&0xef53u16.to_le_bytes());
    image
}

fn object_name(object_no: u64) -> String {
    format!("{OBJECT_PREFIX}.{object_no:016x}")
}
