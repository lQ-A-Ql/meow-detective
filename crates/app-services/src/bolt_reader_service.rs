//! Lifecycle and resource admission for read-only Bolt worker requests.
mod error;
mod job;
mod pipes;
mod process;
mod profile;
mod validation;

use crate::resource_budget::{ResourcePermit, Resources};
pub use error::BoltReadError;
pub use profile::BoltReadMode;
use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use transport::dto::bolt_reader::BoltReadReportDto;

pub struct BoltRead {
    report: BoltReadReportDto,
    _permit: ResourcePermit,
}

impl BoltRead {
    pub fn report(&self) -> &BoltReadReportDto {
        &self.report
    }
}

pub fn decode(
    workspace: &Path,
    bytes: &[u8],
    mode: BoltReadMode,
    cancel: &AtomicBool,
) -> Result<BoltRead, BoltReadError> {
    if cancel.load(Ordering::Acquire) {
        return Err(BoltReadError::Cancelled);
    }
    let mut request = profile::request(mode, bytes.len() as u64);
    if bytes.len() as u64 > request.limits.max_file_bytes {
        return Err(BoltReadError::InputLimit);
    }
    let resources = Resources {
        bytes: request.limits.memory_limit_bytes
            + request.limits.max_output_bytes
            + bytes.len() as u64,
        handles: 16,
        ..Resources::default()
    };
    let permit = crate::resource_budget::global_pool().acquire(resources, None)?;
    std::fs::create_dir_all(workspace)?;
    let mut copy = tempfile::NamedTempFile::new_in(workspace)?;
    copy.write_all(bytes)?;
    copy.flush()?;
    let path = copy.into_temp_path();
    request.input_path = path.to_str().ok_or(BoltReadError::InvalidInput)?.into();
    request.input_sha256 = infrastructure::hashing::sha256_bytes(bytes);
    if cancel.load(Ordering::Acquire) {
        return Err(BoltReadError::Cancelled);
    }
    let report = process::execute(&request, cancel)?;
    let retained = validation::retained_bytes(&report).saturating_add(bytes.len() as u64);
    let permit = permit.shrink(Resources {
        bytes: retained,
        ..Resources::default()
    })?;
    Ok(BoltRead {
        report,
        _permit: permit,
    })
}
