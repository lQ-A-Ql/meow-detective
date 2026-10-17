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
}

impl<F, R> EvidenceReaderFactory for OpenFactory<F>
where
    F: Fn() -> io::Result<R> + Send + Sync + 'static,
    R: EvidenceReader + 'static,
{
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>> {
        self.validate_snapshot()?;
        (self.opener)().map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
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
    match kind {
        DataSourceKind::E01 => {
            let plan = image_e01::E01Reader::open(&path.canonicalize()?)?.open_plan()?;
            let snapshot = super::snapshot::SourceSnapshot::capture(&plan.segment_paths())?;
            let metadata_identity = Some(snapshot.metadata_identity());
            let size = plan.logical_size();
            let plan = Arc::new(plan);
            let opener_plan = Arc::clone(&plan);
            Ok(Arc::new(OpenFactory {
                opener: move || image_e01::E01Reader::from_open_plan(&opener_plan),
                validator: Box::new(move || {
                    snapshot.validate()?;
                    plan.validate_snapshot()
                }),
                metadata_identity,
                size,
                granularity: 512,
                capabilities: ReaderCapabilities {
                    independent_cursor: true,
                    max_concurrent_readers: 4,
                    stable_snapshot: true,
                },
            }))
        }
        DataSourceKind::Raw => {
            let path = path.canonicalize()?;
            let template = evidence_core::RawImageReader::open(&path)?;
            let size = template.info().size;
            let snapshot = super::snapshot::SourceSnapshot::capture(template.backing_paths())?;
            let metadata_identity = Some(snapshot.metadata_identity());
            Ok(Arc::new(OpenFactory {
                opener: move || {
                    let reader = evidence_core::RawImageReader::open(&path)?;
                    if reader.info().size != size {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "source logical size changed since planning",
                        ));
                    }
                    Ok(reader)
                },
                validator: Box::new(move || snapshot.validate()),
                metadata_identity,
                size,
                granularity: 512,
                capabilities: ReaderCapabilities {
                    max_concurrent_readers: 4,
                    ..ReaderCapabilities::default()
                },
            }))
        }
        DataSourceKind::LocalDisk => {
            let template = evidence_core::LocalDiskReader::open(path)?;
            let size = template.info().size;
            let path = path.to_path_buf();
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
            }))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "source kind has no image reader factory",
        )),
    }
}
