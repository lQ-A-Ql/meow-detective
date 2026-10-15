use super::super::{error::ParallelEnumError, partition_work::PartitionWork};
use evidence_core::{EvidenceReader, LocalDiskReader, RawImageReader};
use image_e01::E01Reader;

pub(in crate::parallel_enum) fn open_partition_evidence_reader(
    partition: &PartitionWork,
) -> Result<Box<dyn EvidenceReader>, ParallelEnumError> {
    if let Some(factory) = &partition.reader_factory {
        return factory.open_reader().map_err(ParallelEnumError::Io);
    }
    if partition.uses_e01_reader() {
        return Ok(Box::new(
            E01Reader::open(&partition.source_path).map_err(|error| error.to_string())?,
        ));
    }
    if partition.uses_local_disk_reader() {
        return Ok(Box::new(
            LocalDiskReader::open(&partition.source_path).map_err(|error| error.to_string())?,
        ));
    }
    Ok(Box::new(
        RawImageReader::open(&partition.source_path).map_err(|error| error.to_string())?,
    ))
}
