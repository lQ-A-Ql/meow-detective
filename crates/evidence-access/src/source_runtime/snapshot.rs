//! Metadata identity checks for every physical member of an immutable image view.
use sha2::{Digest, Sha256};
use std::{io, path::PathBuf, time::SystemTime};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceSnapshot(Vec<(PathBuf, u64, SystemTime, Option<SystemTime>)>);
impl SourceSnapshot {
    pub fn paths(&self) -> impl Iterator<Item = &std::path::Path> {
        self.0.iter().map(|(path, ..)| path.as_path())
    }
    pub fn contains(&self, path: &std::path::Path) -> bool {
        self.0.iter().any(|(known, ..)| known == path)
    }
    pub fn merged(snapshots: &[Self]) -> Self {
        let mut members = std::collections::BTreeMap::new();
        for snapshot in snapshots {
            for (path, length, modified, created) in &snapshot.0 {
                members.insert(path.clone(), (*length, *modified, *created));
            }
        }
        Self(
            members
                .into_iter()
                .map(|(path, (length, modified, created))| (path, length, modified, created))
                .collect(),
        )
    }

    pub fn capture_images(
        sources: &[crate::datasource_service::LvmDiscoverySource],
    ) -> io::Result<Self> {
        let mut paths = std::collections::BTreeSet::new();
        for source in sources {
            match source.source_kind {
                domain::DataSourceKind::E01 => paths.extend(
                    super::resources::open_e01_plan(
                        &source.source_path,
                        None,
                        None,
                        Default::default(),
                    )?
                    .segment_paths(),
                ),
                domain::DataSourceKind::Raw => {
                    let _parse = super::resources::raw_parse(None)?;
                    paths.extend(
                        evidence_core::RawImageReader::open(&source.source_path)?
                            .backing_paths()
                            .iter()
                            .cloned(),
                    );
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "image snapshot requires RAW or E01 sources",
                    ))
                }
            }
        }
        Self::capture(&paths.into_iter().collect::<Vec<_>>())
    }

    pub(crate) fn metadata_bytes(&self) -> u64 {
        (self.0.capacity() * std::mem::size_of::<(PathBuf, u64, SystemTime, Option<SystemTime>)>()
            + self.0.iter().map(|e| e.0.capacity()).sum::<usize>()) as u64
    }
    pub fn metadata_identity(&self) -> String {
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
    pub fn capture(paths: &[PathBuf]) -> io::Result<Self> {
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
    pub fn validate(&self) -> io::Result<()> {
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

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
