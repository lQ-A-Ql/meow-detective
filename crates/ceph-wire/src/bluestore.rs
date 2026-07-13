use uuid::Uuid;

use crate::{
    codec::{
        decode_string, decode_string_map, CephDecode, CephEncode, CephStringMap,
        CephStructEnvelope, CephUtime, DEFAULT_MAX_MAP_ENTRIES, DEFAULT_MAX_STRING_LENGTH,
    },
    crc32c::ceph_crc32c,
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub const BDEV_FIRST_LABEL_POSITION: u64 = 0;
pub const BDEV_LABEL_BLOCK_SIZE: usize = 4096;
pub const BDEV_LABEL_PREFIX_LENGTH: usize = 60;
pub const BDEV_LABEL_MAGIC: &[u8] = b"bluestore block device\n";
pub const BDEV_LABEL_POSITIONS: [u64; 5] = [
    BDEV_FIRST_LABEL_POSITION,
    1 << 30,
    10 << 30,
    100 << 30,
    1000 << 30,
];

const LABEL_STRUCT_VERSION: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BdevLabel {
    pub osd_uuid: Uuid,
    pub size: u64,
    pub birth_time: CephUtime,
    pub description: String,
    pub metadata: CephStringMap,
    pub struct_version: u8,
    pub struct_compat_version: u8,
}

impl BdevLabel {
    pub fn is_multi(&self) -> bool {
        self.metadata
            .get("multi")
            .is_some_and(|value| value == "yes")
    }

    pub fn epoch(&self) -> Result<Option<i64>> {
        let Some(value) = self.metadata.get("epoch") else {
            return Ok(None);
        };
        value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| CephWireError::InvalidEpoch {
                value: value.clone(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BdevLabelCandidate {
    pub position: u64,
    pub block: Vec<u8>,
}

impl BdevLabelCandidate {
    pub fn new(position: u64, block: impl Into<Vec<u8>>) -> Self {
        Self {
            position,
            block: block.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BdevLabelSelection {
    pub label: BdevLabel,
    pub valid_positions: Vec<u64>,
    pub is_multi: bool,
    pub epoch: Option<i64>,
}

pub fn decode_bdev_label_block(block: &[u8]) -> Result<BdevLabel> {
    if block.len() < BDEV_LABEL_BLOCK_SIZE {
        return Err(CephWireError::UnexpectedEof {
            offset: 0,
            needed: BDEV_LABEL_BLOCK_SIZE,
            remaining: block.len(),
        });
    }

    let mut cursor = CephCursor::new(&block[..BDEV_LABEL_BLOCK_SIZE]);
    cursor.skip(BDEV_LABEL_PREFIX_LENGTH)?;

    let (envelope, mut payload) =
        CephStructEnvelope::decode_payload(&mut cursor, LABEL_STRUCT_VERSION)?;
    let osd_uuid = Uuid::decode(&mut payload)?;
    let size = u64::decode(&mut payload)?;
    let birth_time = CephUtime::decode(&mut payload)?;
    let description = decode_string(&mut payload, DEFAULT_MAX_STRING_LENGTH, "label description")?;
    let metadata = if envelope.version >= 2 {
        decode_string_map(
            &mut payload,
            DEFAULT_MAX_MAP_ENTRIES,
            DEFAULT_MAX_STRING_LENGTH,
        )?
    } else {
        CephStringMap::new()
    };

    if payload.position() > envelope.payload_length as usize {
        return Err(CephWireError::StructBoundaryExceeded {
            struct_end: envelope.payload_length as usize,
            offset: payload.position(),
        });
    }
    if !payload.is_empty() {
        payload.skip(payload.remaining())?;
    }

    let crc_offset = cursor.position();
    let expected_crc = u32::decode(&mut cursor)?;
    let actual_crc = ceph_crc32c(&block[..crc_offset]);
    if expected_crc != actual_crc {
        return Err(CephWireError::CrcMismatch {
            expected: expected_crc,
            actual: actual_crc,
        });
    }

    Ok(BdevLabel {
        osd_uuid,
        size,
        birth_time,
        description,
        metadata,
        struct_version: envelope.version,
        struct_compat_version: envelope.compat_version,
    })
}

pub fn select_bdev_labels(
    candidates: impl IntoIterator<Item = BdevLabelCandidate>,
    requested_uuid: Option<Uuid>,
) -> Result<BdevLabelSelection> {
    let decoded = candidates
        .into_iter()
        .filter_map(|candidate| {
            decode_bdev_label_block(&candidate.block)
                .ok()
                .map(|label| (candidate.position, label))
        })
        .collect::<Vec<_>>();
    select_bdev_label(decoded, requested_uuid)
}

pub fn select_bdev_label(
    labels: impl IntoIterator<Item = (u64, BdevLabel)>,
    requested_uuid: Option<Uuid>,
) -> Result<BdevLabelSelection> {
    let mut labels = labels.into_iter().collect::<Vec<_>>();
    labels.sort_by_key(|(position, _)| *position);

    let mut locked_uuid = requested_uuid;
    let mut selected: Option<BdevLabel> = None;
    let mut selected_epoch = None;
    let mut selected_position = None;
    let mut valid_positions = Vec::new();

    for (position, label) in labels {
        if locked_uuid.is_some_and(|uuid| label.osd_uuid != uuid) {
            continue;
        }
        if position == BDEV_FIRST_LABEL_POSITION && !label.is_multi() {
            return Ok(BdevLabelSelection {
                label,
                valid_positions: vec![position],
                is_multi: false,
                epoch: None,
            });
        }
        if !label.is_multi() {
            continue;
        }

        let Some(epoch) = label.epoch()? else {
            continue;
        };
        if locked_uuid.is_none() {
            locked_uuid = Some(label.osd_uuid);
        }
        if locked_uuid != Some(label.osd_uuid) {
            continue;
        }

        match selected_epoch {
            None => {
                selected_epoch = Some(epoch);
                selected = Some(label);
                selected_position = Some(position);
                valid_positions.clear();
                valid_positions.push(position);
            }
            Some(current) if epoch > current => {
                selected_epoch = Some(epoch);
                selected = Some(label);
                selected_position = Some(position);
                valid_positions.clear();
                valid_positions.push(position);
            }
            Some(current) if epoch == current => {
                let current_label = selected.as_ref().expect("selected label for current epoch");
                if current_label != &label {
                    return Err(CephWireError::ConflictingLabelCopies {
                        osd_uuid: label.osd_uuid,
                        epoch,
                        first_position: selected_position
                            .expect("selected position for current epoch"),
                        conflicting_position: position,
                    });
                }
                valid_positions.push(position);
            }
            Some(_) => {}
        }
    }

    let label = selected.ok_or(CephWireError::NoValidLabel)?;
    Ok(BdevLabelSelection {
        label,
        valid_positions,
        is_multi: true,
        epoch: selected_epoch,
    })
}

#[doc(hidden)]
pub fn encode_bdev_label_block(label: &BdevLabel) -> Vec<u8> {
    let mut output = Vec::with_capacity(BDEV_LABEL_BLOCK_SIZE);
    output.extend_from_slice(BDEV_LABEL_MAGIC);
    output.extend_from_slice(label.osd_uuid.hyphenated().to_string().as_bytes());
    output.push(b'\n');

    let mut payload = Vec::new();
    label.osd_uuid.encode(&mut payload);
    label.size.encode(&mut payload);
    label.birth_time.encode(&mut payload);
    label.description.encode(&mut payload);
    if label.struct_version >= 2 {
        label.metadata.encode(&mut payload);
    }
    CephStructEnvelope {
        version: label.struct_version,
        compat_version: label.struct_compat_version,
        payload_length: payload.len() as u32,
    }
    .encode(&mut output);
    output.extend_from_slice(&payload);
    let crc = ceph_crc32c(&output);
    crc.encode(&mut output);
    output.resize(BDEV_LABEL_BLOCK_SIZE, 0);
    output
}
