use std::io;
use std::path::Path;
use std::sync::Arc;

use domain::DataSourceKind;
use evidence_core::{
    EvidenceReader, EvidenceReaderFactory, ReaderCapabilities, SharedReaderFactory,
};

struct OpenFactory<F> {
    opener: F,
    validator: Box<dyn Fn() -> io::Result<()> + Send + Sync>,
    metadata_identity: Option<String>,
    size: u64,
    granularity: usize,
    capabilities: ReaderCapabilities,
    route: super::resources::ResourceRoute,
    reader_bytes: u64,
    resources: Option<(
        super::resources::ReaderBudget,
        Arc<dyn evidence_core::reader::ReaderResourceHold>,
    )>,
}

impl<F, R> EvidenceReaderFactory for OpenFactory<F>
where
    F: Fn() -> io::Result<R> + Send + Sync + 'static,
    R: EvidenceReader + 'static,
{
    fn bind_reader_scope(&self, scope: Arc<dyn evidence_core::reader::ReaderAllocationScope>) {
        self.route.bind(scope);
    }
    fn reader_memory_bound(&self) -> u64 {
        self.reader_bytes
    }
    fn reader_scope(&self) -> Option<Arc<dyn evidence_core::reader::ReaderAllocationScope>> {
        self.route.scope()
    }
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>> {
        let hold = self
            .resources
            .as_ref()
            .map(|(budget, _)| evidence_core::reader::ReaderAdmission::acquire(budget))
            .transpose()?;
        let reader = Box::new((self.opener)()?) as Box<dyn EvidenceReader>;
        if let (Some(hold), Some((_, metadata))) = (hold, &self.resources) {
            Ok(Box::new(super::resources::BudgetedReader {
                reader,
                _hold: hold,
                _metadata: metadata.clone(),
            }))
        } else {
            Ok(reader)
        }
    }
    fn logical_size(&self) -> u64 {
        self.size
    }
    fn preferred_read_granularity(&self) -> usize {
        self.granularity
    }
    fn capabilities(&self) -> ReaderCapabilities {
        self.capabilities
    }
    fn validate_snapshot(&self) -> io::Result<()> {
        (self.validator)().map_err(|error| io::Error::new(error.kind(), super::SourceInvalidated))
    }
    fn metadata_identity(&self) -> Option<&str> {
        self.metadata_identity.as_deref()
    }
}

pub fn open_factory(path: &Path, kind: &DataSourceKind) -> io::Result<SharedReaderFactory> {
    open_factory_with_cancel(path, kind, &std::sync::atomic::AtomicBool::new(false))
}
pub fn open_factory_with_cancel(
    path: &Path,
    kind: &DataSourceKind,
    cancel: &std::sync::atomic::AtomicBool,
) -> io::Result<SharedReaderFactory> {
    match kind {
        DataSourceKind::E01 => e01_factory(path, cancel, None),
        DataSourceKind::Raw => raw_factory(path, cancel),
        DataSourceKind::LocalDisk => device_factory(path, cancel),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "source kind has no image reader factory",
        )),
    }
}

pub(crate) fn open_factory_for_case(
    path: &Path,
    kind: &DataSourceKind,
    cancel: &std::sync::atomic::AtomicBool,
    case: &str,
) -> io::Result<SharedReaderFactory> {
    if *kind == DataSourceKind::E01 {
        e01_factory(path, cancel, Some(case))
    } else {
        open_factory_with_cancel(path, kind, cancel)
    }
}

fn e01_factory(
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    case: Option<&str>,
) -> io::Result<SharedReaderFactory> {
    let path = path.canonicalize()?;
    let route = super::resources::ResourceRoute::default();
    let plan = super::resources::open_e01_plan(&path, Some(cancel), None, route.clone())?;
    let reader_bytes = plan.reader_memory_bound();
    let snapshot = super::snapshot::SourceSnapshot::capture(&plan.segment_paths())?;
    let metadata_identity = Some(snapshot.metadata_identity());
    let size = plan.logical_size();
    let plan = Arc::new(super::resources::SharedPlan {
        plan,
        route: route.clone(),
    });
    if let Some(case) = case {
        crate::e01_reader_cache::register_factory_plan(case, &path, &plan);
    }
    let opener_plan = Arc::clone(&plan);
    Ok(Arc::new(OpenFactory {
        opener: move || image_e01::E01Reader::from_open_plan(&opener_plan.plan),
        validator: Box::new(move || {
            snapshot.validate()?;
            plan.plan.validate_snapshot()
        }),
        metadata_identity,
        size,
        granularity: 512,
        capabilities: ReaderCapabilities {
            independent_cursor: true,
            max_concurrent_readers: 4,
            stable_snapshot: true,
        },
        route,
        reader_bytes,
        resources: None,
    }))
}

fn raw_factory(
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> io::Result<SharedReaderFactory> {
    let parse = super::resources::raw_parse(Some(cancel))?;
    let path = path.canonicalize()?;
    let template = evidence_core::RawImageReader::open(&path)?;
    let size = template.info().size;
    let snapshot = super::snapshot::SourceSnapshot::capture(template.backing_paths())?;
    let handles = template.backing_paths().len() as u64;
    let reader_bytes = template
        .metadata_bytes()
        .saturating_mul(2)
        .saturating_add(1024 * 1024);
    if reader_bytes > 192 * crate::resource_budget::MIB {
        return Err(crate::resource_budget::ResourceError::Limit.into_io());
    }
    let metadata_bytes = snapshot.metadata_bytes().max(65536);
    let readers = if template.info().kind == "raw" { 4 } else { 1 };
    drop(template);
    let metadata = Arc::new(
        parse
            .shrink(crate::resource_budget::Resources {
                bytes: metadata_bytes,
                metadata: metadata_bytes,
                ..Default::default()
            })
            .map_err(crate::resource_budget::ResourceError::into_io)?,
    );
    let metadata_identity = Some(snapshot.metadata_identity());
    let route = super::resources::ResourceRoute::default();
    let opener_snapshot = snapshot.clone();
    Ok(Arc::new(OpenFactory {
        opener: move || {
            opener_snapshot.validate()?;
            let reader = evidence_core::RawImageReader::open(&path)?;
            if reader.info().size != size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    super::SourceInvalidated,
                ));
            }
            Ok(reader)
        },
        validator: Box::new(move || snapshot.validate()),
        metadata_identity,
        size,
        granularity: 512,
        capabilities: ReaderCapabilities {
            max_concurrent_readers: readers,
            ..ReaderCapabilities::default()
        },
        route: route.clone(),
        reader_bytes,
        resources: Some((
            super::resources::ReaderBudget {
                bytes: reader_bytes,
                handles,
                route,
            },
            metadata,
        )),
    }))
}

fn device_factory(
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> io::Result<SharedReaderFactory> {
    let parse = super::resources::raw_parse(Some(cancel))?;
    let template = evidence_core::LocalDiskReader::open(path)?;
    let size = template.info().size;
    let path = path.to_path_buf();
    drop(template);
    let metadata = Arc::new(
        parse
            .shrink(crate::resource_budget::Resources {
                bytes: 4096,
                metadata: 4096,
                ..Default::default()
            })
            .map_err(crate::resource_budget::ResourceError::into_io)?,
    );
    let route = super::resources::ResourceRoute::default();
    Ok(Arc::new(OpenFactory {
        opener: move || evidence_core::LocalDiskReader::open(&path),
        // A live device has no immutable metadata snapshot guarantee.
        validator: Box::new(|| Ok(())),
        metadata_identity: None,
        size,
        granularity: 512,
        capabilities: ReaderCapabilities {
            independent_cursor: true,
            max_concurrent_readers: 1,
            stable_snapshot: false,
        },
        route: route.clone(),
        reader_bytes: 1024 * 1024,
        resources: Some((
            super::resources::ReaderBudget {
                route,
                bytes: 1024 * 1024,
                handles: 1,
            },
            metadata,
        )),
    }))
}
