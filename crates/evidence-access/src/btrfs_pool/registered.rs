use super::{BtrfsPoolPlan, BtrfsSourceError};
use domain::{structure::SourceRevisionRef, DataSourceId, DataSourceKind};
use evidence_core::{EvidenceReader, FileSystemReader, ReaderInfo};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
    storage_object_repo::StorageObjectRepo,
};
use source_kernel::source_runtime::read_scope::SourceReadScope;
use std::{
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    sync::atomic::AtomicBool,
};

pub fn validate_registered(
    conn: &rusqlite::Connection,
    source: &SourceRevisionRef,
    plan: &BtrfsPoolPlan,
) -> Result<(), BtrfsSourceError> {
    SourceRevisionRepo::new(conn).require_current(source)?;
    let inputs = SourceRevisionRepo::new(conn).inputs(&source.case_id, &source.source_id)?;
    validate_inputs(conn, &source.case_id, plan, &inputs)?;
    let version = StorageObjectRepo::new(conn)
        .version_for_source(&source.case_id, &source.source_id)?
        .ok_or(BtrfsSourceError::Identity)?;
    let native = &plan.identity.devices[0];
    let expected = domain::structure::NativeVersion::Btrfs {
        fsid: native.fsid.clone(),
        generation: native.generation,
        device_count: native.device_count,
        mapping_sha256: plan.identity.mapping_sha256.clone(),
    };
    if version.mapping_sha256 != plan.fingerprint()?
        || version.adapter_version != super::ADAPTER_VERSION
        || version.policy_version != super::POLICY_VERSION
        || version.version_kind != domain::structure::StorageObjectVersionKind::DerivedOutput
        || version.consistency != domain::structure::RevisionConsistency::Coherent
        || version.native_versions.as_slice() != [expected]
        || version.derived_source_id != source.source_id
        || version
            .parent_source_revisions
            .iter()
            .map(|parent| (&parent.case_id, &parent.source_id, parent.revision))
            .collect::<std::collections::BTreeSet<_>>()
            != inputs
                .iter()
                .map(|parent| (&parent.case_id, &parent.source_id, parent.revision))
                .collect::<std::collections::BTreeSet<_>>()
    {
        return Err(BtrfsSourceError::Identity);
    }
    Ok(())
}

pub fn validate_inputs(
    conn: &rusqlite::Connection,
    case_id: &str,
    plan: &BtrfsPoolPlan,
    inputs: &[SourceRevisionRef],
) -> Result<(), BtrfsSourceError> {
    plan.validate()?;
    let sources = DataSourceRepo::new(conn);
    let mut registered = std::collections::BTreeMap::new();
    for input in inputs {
        if input.case_id != case_id {
            return Err(BtrfsSourceError::Identity);
        }
        SourceRevisionRepo::new(conn).require_current(input)?;
        let path =
            std::path::PathBuf::from(sources.source_path(&DataSourceId(input.source_id.clone()))?)
                .canonicalize()?;
        if registered.insert(path, input).is_some() {
            return Err(BtrfsSourceError::Identity);
        }
    }
    let mut used = std::collections::BTreeSet::new();
    for member in &plan.members {
        let path = Path::new(&member.content.source_path).canonicalize()?;
        let input = registered.get(&path).ok_or(BtrfsSourceError::Identity)?;
        used.insert(path);
        let (_, known) = SourceRevisionRepo::new(conn).current(case_id, &input.source_id)?;
        if known.logical_size != member.content.logical_size
            || known.logical_sha256.as_deref() != Some(member.content.logical_sha256.as_str())
            || sources.source_kind(&DataSourceId(input.source_id.clone()))?
                != member.content.source_kind
        {
            return Err(BtrfsSourceError::Identity);
        }
    }
    if used.len() != inputs.len() {
        return Err(BtrfsSourceError::Identity);
    }
    Ok(())
}

pub(crate) fn open_registered_filesystem(
    conn: &rusqlite::Connection,
    root: &Path,
    source: &SourceRevisionRef,
) -> Result<(fs_btrfs::BtrfsReader, SourceReadScope), BtrfsSourceError> {
    let id = DataSourceId(source.source_id.clone());
    if DataSourceRepo::new(conn).source_kind(&id)? != DataSourceKind::BtrfsPool {
        return Err(BtrfsSourceError::Identity);
    }
    let scope = SourceReadScope::acquire(root, source)?;
    let plan = super::read_descriptor(Path::new(&DataSourceRepo::new(conn).source_path(&id)?))?;
    validate_registered(conn, source, &plan)?;
    let filesystem = super::open_filesystem(&plan, &AtomicBool::new(false))?;
    Ok((filesystem, scope))
}

pub fn open_file(
    conn: &rusqlite::Connection,
    root: &Path,
    source: &SourceRevisionRef,
    path: &str,
    size: u64,
) -> Result<Box<dyn EvidenceReader>, BtrfsSourceError> {
    let (filesystem, scope) = open_registered_filesystem(conn, root, source)?;
    scope
        .wrap(Box::new(PoolFileReader {
            filesystem,
            path: path.into(),
            position: 0,
            info: ReaderInfo {
                path: "btrfs-file".into(),
                size,
                kind: "btrfs_pool".into(),
            },
        }))
        .map_err(Into::into)
}

struct PoolFileReader {
    filesystem: fs_btrfs::BtrfsReader,
    path: String,
    position: u64,
    info: ReaderInfo,
}
impl Read for PoolFileReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let bytes = self
            .filesystem
            .read_file_range(&self.path, self.position, output.len())?;
        output[..bytes.len()].copy_from_slice(&bytes);
        self.position += bytes.len() as u64;
        Ok(bytes.len())
    }
}
impl Seek for PoolFileReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
            SeekFrom::End(delta) => self.info.size.checked_add_signed(delta),
        }
        .filter(|position| *position <= self.info.size)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Btrfs file seek exceeds file bounds",
            )
        })?;
        Ok(self.position)
    }
}
impl EvidenceReader for PoolFileReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
