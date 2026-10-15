//! Metadata identity checks for every physical member of an immutable image view.
use std::{io, path::PathBuf, time::SystemTime};

pub(super) struct SourceSnapshot(Vec<(PathBuf, u64, SystemTime)>);
impl SourceSnapshot {
    pub(super) fn capture(paths: &[PathBuf]) -> io::Result<Self> {
        paths
            .iter()
            .map(|path| {
                let metadata = std::fs::metadata(path)?;
                Ok((path.clone(), metadata.len(), metadata.modified()?))
            })
            .collect::<io::Result<Vec<_>>>()
            .map(Self)
    }
    pub(super) fn validate(&self) -> io::Result<()> {
        for (path, length, modified) in &self.0 {
            let metadata = std::fs::metadata(path)?;
            if metadata.len() != *length || metadata.modified()? != *modified {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "source member changed since planning",
                ));
            }
        }
        Ok(())
    }
}
