use domain::DataSourceKind;
use std::path::Path;

pub(crate) fn open_evidence_reader(
    source_path: &Path,
    source_kind: &DataSourceKind,
) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>> {
    match source_kind {
        DataSourceKind::E01 => image_e01::E01Reader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>),
        DataSourceKind::Raw => evidence_core::RawImageReader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>),
        DataSourceKind::LogicalDirectory | DataSourceKind::CephRbd | DataSourceKind::CephFs => {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("data source kind {source_kind} is not a host image reader"),
            ))
        }
    }
}
