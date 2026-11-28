use crate::{
    datasource_service::LvmPhysicalVolumeSource,
    source_runtime::{
        content::{ContentVerifiedReader, SourceContent},
        snapshot::SourceSnapshot,
    },
};
use evidence_core::EvidenceReader;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read, Seek},
    ops::Deref,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, LazyLock,
    },
    time::Duration,
};

static DISCOVERIES: AtomicU64 = AtomicU64::new(0);
static CACHE: LazyLock<moka::sync::Cache<String, Arc<Mapping>>> = LazyLock::new(|| {
    moka::sync::Cache::builder()
        .max_capacity(128 * 1024 * 1024)
        .weigher(|_: &String, mapping: &Arc<Mapping>| mapping.bytes.min(u32::MAX as u64) as u32)
        .time_to_idle(Duration::from_secs(60))
        .build()
});

struct Mapping {
    metadata: fs_lvm::LvmPoolPlan,
    snapshot: SourceSnapshot,
    inputs: Vec<(u64, String, SourceContent)>,
    bytes: u64,
    _permit: source_kernel::resource_budget::ResourcePermit,
}

pub struct SharedLvmPool {
    pool: fs_lvm::LvmPool,
    mapping: Arc<Mapping>,
    _permit: source_kernel::resource_budget::ResourcePermit,
}
impl Deref for SharedLvmPool {
    type Target = fs_lvm::LvmPool;
    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}
impl SharedLvmPool {
    pub(crate) fn from_pool(pool: fs_lvm::LvmPool) -> io::Result<Arc<Self>> {
        let bytes = 1024 * 1024;
        let mapping = Arc::new(Mapping {
            metadata: pool.metadata_plan(),
            snapshot: SourceSnapshot::default(),
            inputs: Vec::new(),
            bytes,
            _permit: reserve(bytes, None)?,
        });
        Ok(Arc::new(Self {
            pool,
            mapping,
            _permit: reserve(bytes, None)?,
        }))
    }
    pub fn open_volume(self: &Arc<Self>, index: usize) -> io::Result<Box<dyn EvidenceReader>> {
        self.mapping.snapshot.validate()?;
        Ok(Box::new(HeldVolume {
            inner: self
                .pool
                .open_volume_reader(index)
                .map_err(io::Error::other)?,
            pool: self.clone(),
        }))
    }
}
pub fn mapping_discovery_count() -> u64 {
    DISCOVERIES.load(Ordering::Acquire)
}

pub(crate) fn open(
    sources: &[LvmPhysicalVolumeSource],
    content: impl Fn(&LvmPhysicalVolumeSource) -> io::Result<SourceContent>,
    cancel: &AtomicBool,
    scope: Option<Arc<dyn evidence_core::reader::ReaderAllocationScope>>,
) -> io::Result<Arc<SharedLvmPool>> {
    let mut inputs = sources
        .iter()
        .map(|source| {
            let content = content(source)?;
            content.validate().map_err(io::Error::other)?;
            Ok((source.offset, source.pv_uuid.clone(), content))
        })
        .collect::<io::Result<Vec<_>>>()?;
    inputs
        .sort_by(|left, right| (&left.2.source_path, left.0).cmp(&(&right.2.source_path, right.0)));
    let discovery = inputs
        .iter()
        .map(|(_, _, content)| {
            crate::datasource_service::LvmDiscoverySource::new(
                &content.source_path,
                content.source_kind.clone(),
            )
        })
        .collect::<Vec<_>>();
    let snapshot = SourceSnapshot::capture_images(&discovery)?;
    let payload =
        serde_json::to_vec(&(&inputs, &snapshot, "lvm-mapping-v1")).map_err(io::Error::other)?;
    let key = hex::encode(Sha256::digest(&payload));
    let bytes = payload.len() as u64 * 2 + inputs.len() as u64 * 1024 * 1024 + 1024 * 1024;
    let mapping = CACHE
        .try_get_with(key, || {
            check_cancel(cancel)?;
            let permit = reserve(bytes, Some(cancel))?;
            let readers = open_inputs(&inputs, cancel, None)?;
            let pool = fs_lvm::LvmPool::discover(
                readers,
                inputs.iter().map(|(offset, ..)| *offset).collect(),
            )
            .map_err(io::Error::other)?;
            snapshot.validate()?;
            DISCOVERIES.fetch_add(1, Ordering::AcqRel);
            Ok(Arc::new(Mapping {
                metadata: pool.metadata_plan(),
                snapshot,
                inputs,
                bytes,
                _permit: permit,
            }))
        })
        .map_err(|error: Arc<io::Error>| io::Error::new(error.kind(), error.to_string()))?;
    check_cancel(cancel)?;
    mapping.snapshot.validate()?;
    let permit = reserve(mapping.bytes, Some(cancel))?;
    let pool = mapping
        .metadata
        .reopen(
            open_inputs(&mapping.inputs, cancel, scope)?,
            mapping.inputs.iter().map(|(offset, ..)| *offset).collect(),
        )
        .map_err(io::Error::other)?;
    Ok(Arc::new(SharedLvmPool {
        pool,
        mapping,
        _permit: permit,
    }))
}
fn reserve(
    bytes: u64,
    cancel: Option<&AtomicBool>,
) -> io::Result<source_kernel::resource_budget::ResourcePermit> {
    source_kernel::resource_budget::global_pool()
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes,
                ..Default::default()
            },
            cancel,
        )
        .map_err(source_kernel::resource_budget::ResourceError::into_io)
}
fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "LVM mapping cancelled",
        ));
    }
    Ok(())
}
fn open_inputs(
    inputs: &[(u64, String, SourceContent)],
    cancel: &AtomicBool,
    scope: Option<Arc<dyn evidence_core::reader::ReaderAllocationScope>>,
) -> io::Result<Vec<Box<dyn EvidenceReader>>> {
    inputs
        .iter()
        .map(|(_, _, content)| {
            let factory = crate::source_runtime::open_factory_with_cancel(
                Path::new(&content.source_path),
                &content.source_kind,
                cancel,
            )?;
            if let Some(scope) = &scope {
                factory.bind_reader_scope(scope.clone());
            }
            Ok(Box::new(ContentVerifiedReader::new(
                factory.open_reader_with_cancel(cancel)?,
                Arc::new(content.clone()),
            )?) as Box<dyn EvidenceReader>)
        })
        .collect()
}
struct HeldVolume {
    inner: Box<dyn EvidenceReader>,
    pool: Arc<SharedLvmPool>,
}
impl Read for HeldVolume {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.pool.mapping.snapshot.validate()?;
        self.inner.read(bytes)
    }
}
impl Seek for HeldVolume {
    fn seek(&mut self, target: io::SeekFrom) -> io::Result<u64> {
        self.pool.mapping.snapshot.validate()?;
        self.inner.seek(target)
    }
}
impl EvidenceReader for HeldVolume {
    fn info(&self) -> &evidence_core::ReaderInfo {
        self.inner.info()
    }
    fn preferred_read_granularity(&self) -> usize {
        self.inner.preferred_read_granularity()
    }
    fn reserve_work(
        &self,
        bytes: u64,
    ) -> io::Result<Option<Arc<dyn evidence_core::reader::ReaderResourceHold>>> {
        self.inner.reserve_work(bytes)
    }
}
