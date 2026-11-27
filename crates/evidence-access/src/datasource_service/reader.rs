use domain::DataSourceKind;
use std::path::Path;

pub fn open_evidence_reader(
    source_path: &Path,
    source_kind: &DataSourceKind,
) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>> {
    match source_kind {
        DataSourceKind::E01 => image_e01::E01Reader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>),
        DataSourceKind::Raw => evidence_core::RawImageReader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>),
        DataSourceKind::LocalDisk => evidence_core::LocalDiskReader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>),
        DataSourceKind::AndroidSparse => image_android::AndroidSparseReader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            .map_err(std::io::Error::other),
        DataSourceKind::MdRaid => crate::mdraid::open_persisted_mdraid_reader(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            .map_err(std::io::Error::other),
        DataSourceKind::LvmLogicalVolume => {
            let plan =
                crate::lvm::read_lvm_descriptor(source_path).map_err(std::io::Error::other)?;
            crate::lvm::open_lvm_volume_reader(&plan, &std::sync::atomic::AtomicBool::new(false))
                .map_err(std::io::Error::other)
        }
        DataSourceKind::LogicalDirectory
        | DataSourceKind::LogicalArchive
        | DataSourceKind::CephRbd
        | DataSourceKind::CephFs
        | DataSourceKind::NestedRange => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("data source kind {source_kind} is not a host image reader"),
        )),
    }
}
