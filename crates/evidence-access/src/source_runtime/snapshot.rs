//! Metadata identity checks for every physical member of an immutable image view.
use sha2::{Digest, Sha256};
use std::{io, path::PathBuf, time::SystemTime};

#[derive(Clone)]
pub(super) struct SourceSnapshot(Vec<(PathBuf, u64, SystemTime, Option<SystemTime>)>);
impl SourceSnapshot {
    pub(super) fn metadata_bytes(&self) -> u64 {
        (self.0.capacity() * std::mem::size_of::<(PathBuf, u64, SystemTime, Option<SystemTime>)>()
            + self.0.iter().map(|e| e.0.capacity()).sum::<usize>()) as u64
    }
    pub(super) fn metadata_identity(&self) -> String {
        let mut digest = Sha256::new();
        for (path, length, modified, created) in &self.0 {
            let path = path.as_os_str().as_encoded_bytes();
            digest.update((path.len() as u64).to_le_bytes());
            digest.update(path);
            digest.update(length.to_le_bytes());
            hash_time(&mut digest, *modified);
            digest.update([u8::from(created.is_some())]);
            if let Some(created) = created {
                hash_time(&mut digest, *created);
            }
        }
        format!("metadata-v1:{}", hex::encode(digest.finalize()))
    }
    pub(super) fn capture(paths: &[PathBuf]) -> io::Result<Self> {
        paths
            .iter()
            .map(|path| {
                let metadata = std::fs::metadata(path)?;
                Ok((
                    path.canonicalize()?,
                    metadata.len(),
                    metadata.modified()?,
                    metadata.created().ok(),
                ))
            })
            .collect::<io::Result<Vec<_>>>()
            .map(Self)
    }
    pub(super) fn validate(&self) -> io::Result<()> {
        for (path, length, modified, created) in &self.0 {
            let metadata = std::fs::metadata(path)?;
            if metadata.len() != *length
                || metadata.modified()? != *modified
                || metadata.created().ok() != *created
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "source member changed since planning",
                ));
            }
        }
        Ok(())
    }
}

fn hash_time(digest: &mut Sha256, time: SystemTime) {
    let (before, duration) = match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => (false, duration),
        Err(error) => (true, error.duration()),
    };
    digest.update([u8::from(before)]);
    digest.update(duration.as_secs().to_le_bytes());
    digest.update(duration.subsec_nanos().to_le_bytes());
}
