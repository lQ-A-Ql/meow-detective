use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, ReaderInfo};
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::CephBluestoreObjectReadPlan;
use thiserror::Error;

mod plan;
use plan::{build_blob_plans, build_logical_extents, BlobPlan, LogicalExtent};

#[derive(Debug, Error)]
pub enum RadosReadError {
    #[error("invalid BlueStore object read plan: {0}")]
    InvalidPlan(String),
    #[error("unsupported BlueStore object layout: {0}")]
    Unsupported(String),
}

pub struct RadosObjectReader {
    device: Box<dyn EvidenceReader>,
    info: ReaderInfo,
    position: u64,
    object_size: u64,
    logical_extents: Vec<LogicalExtent>,
    blobs: BTreeMap<u32, BlobPlan>,
}

impl RadosObjectReader {
    pub fn new(
        device: Box<dyn EvidenceReader>,
        plan: CephBluestoreObjectReadPlan,
    ) -> Result<Self, RadosReadError> {
        validate_plan_identity(&plan)?;
        let blobs = build_blob_plans(&plan)?;
        let logical_extents = build_logical_extents(&plan, &blobs)?;
        let object_size = plan.object.size;
        Ok(Self {
            device,
            info: ReaderInfo {
                path: PathBuf::from(format!("ceph-rados/{}", plan.object_identity_sha256)),
                size: object_size,
                kind: "ceph-rados-object".to_string(),
            },
            position: 0,
            object_size,
            logical_extents,
            blobs,
        })
    }

    fn read_at(&mut self, offset: u64, output: &mut [u8]) -> io::Result<usize> {
        let available = self.object_size.saturating_sub(offset);
        let length = usize::try_from(available.min(output.len() as u64))
            .map_err(|_| invalid_data("object range length exceeds usize"))?;
        let output = &mut output[..length];
        output.fill(0);
        if output.is_empty() {
            return Ok(0);
        }
        let end = offset
            .checked_add(length as u64)
            .ok_or_else(|| invalid_data("object range end overflow"))?;
        let extents = self
            .logical_extents
            .iter()
            .filter(|extent| extent.logical_offset < end && extent.end > offset)
            .cloned()
            .collect::<Vec<_>>();
        for extent in extents {
            let start = offset.max(extent.logical_offset);
            let extent_end = end.min(extent.end);
            let destination = usize::try_from(start - offset)
                .map_err(|_| invalid_data("destination offset exceeds usize"))?;
            let blob_offset = extent
                .blob_offset
                .checked_add(start - extent.logical_offset)
                .ok_or_else(|| invalid_data("blob read offset overflow"))?;
            let read_length = usize::try_from(extent_end - start)
                .map_err(|_| invalid_data("blob read length exceeds usize"))?;
            self.read_blob(
                extent.blob_ordinal,
                blob_offset,
                &mut output[destination..][..read_length],
            )?;
        }
        Ok(length)
    }

    fn read_blob(&mut self, blob_ordinal: u32, offset: u64, output: &mut [u8]) -> io::Result<()> {
        let blob = self
            .blobs
            .get(&blob_ordinal)
            .cloned()
            .ok_or_else(|| invalid_data("logical extent references a missing blob"))?;
        blob.read_range(self.device.as_mut(), offset, output)?;
        blob.verify_overlapping_checksums(self.device.as_mut(), offset, output)
    }
}

impl Read for RadosObjectReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let read = self.read_at(self.position, output)?;
        self.position = self
            .position
            .checked_add(read as u64)
            .ok_or_else(|| invalid_data("object reader position overflow"))?;
        Ok(read)
    }
}

impl Seek for RadosObjectReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.object_size) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
        };
        if !(0..=i128::from(self.object_size)).contains(&next) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "RADOS object seek lies outside the object",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

impl EvidenceReader for RadosObjectReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn validate_plan_identity(plan: &CephBluestoreObjectReadPlan) -> Result<(), RadosReadError> {
    let inventory_id = plan.inventory_id.as_str();
    let object_identity = plan.object_identity_sha256.as_str();
    if plan.object.inventory_id != inventory_id
        || plan.object.object_identity_sha256 != object_identity
        || plan.object.decode_status != "parsed"
        || plan.object.deferred_reason.is_some()
    {
        return Err(RadosReadError::InvalidPlan(
            "object identity or decode status is not bound to the read plan".to_string(),
        ));
    }
    if !is_canonical_sha256(object_identity) {
        return Err(RadosReadError::InvalidPlan(
            "object identity is not canonical lowercase SHA-256".to_string(),
        ));
    }
    for blob in &plan.blobs {
        if blob.inventory_id != inventory_id || blob.object_identity_sha256 != object_identity {
            return Err(RadosReadError::InvalidPlan(
                "blob identity is not bound to the read plan".to_string(),
            ));
        }
    }
    for extent in &plan.logical_extents {
        if extent.inventory_id != inventory_id || extent.object_identity_sha256 != object_identity {
            return Err(RadosReadError::InvalidPlan(
                "logical extent identity is not bound to the read plan".to_string(),
            ));
        }
    }
    for extent in &plan.physical_extents {
        if extent.inventory_id != inventory_id || extent.object_identity_sha256 != object_identity {
            return Err(RadosReadError::InvalidPlan(
                "physical extent identity is not bound to the read plan".to_string(),
            ));
        }
    }
    Ok(())
}

fn is_canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/rados_reader.rs"]
mod tests;
