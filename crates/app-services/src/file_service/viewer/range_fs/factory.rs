use evidence_core::FileSystemReader;

use crate::file_service::{viewer::PreviewPartitionCandidate, FileServiceError};

pub(crate) fn open_filesystem_reader(
    candidate: &PreviewPartitionCandidate,
    reader: Box<dyn evidence_core::EvidenceReader>,
    filesystem_offset: u64,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    let kind = candidate.filesystem_kind.as_str();
    let filesystem = match kind {
        kind if kind.eq_ignore_ascii_case("NTFS") => {
            fs_ntfs::NtfsReader::open(reader, filesystem_offset)
                .map(|fs| Box::new(fs) as Box<dyn FileSystemReader + Send>)
        }
        kind if kind.eq_ignore_ascii_case("FAT") => {
            fs_fat::FatReader::open(reader, filesystem_offset)
                .map(|fs| Box::new(fs) as Box<dyn FileSystemReader + Send>)
        }
        kind if kind.eq_ignore_ascii_case("EXFAT") => {
            fs_exfat::ExfatReader::open(reader, filesystem_offset)
                .map(|fs| Box::new(fs) as Box<dyn FileSystemReader + Send>)
        }
        kind if kind.eq_ignore_ascii_case("EXT4") => {
            fs_ext4::Ext4Reader::open(reader, filesystem_offset)
                .map(|fs| Box::new(fs) as Box<dyn FileSystemReader + Send>)
        }
        kind if kind.eq_ignore_ascii_case("XFS") => {
            fs_xfs::XfsReader::open(reader, filesystem_offset)
                .map(|fs| Box::new(fs) as Box<dyn FileSystemReader + Send>)
        }
        kind if kind.eq_ignore_ascii_case("BTRFS") => {
            fs_btrfs::BtrfsReader::open(reader, filesystem_offset)
                .map(|fs| Box::new(fs) as Box<dyn FileSystemReader + Send>)
        }
        _ => {
            return Err(FileServiceError::Unsupported(format!(
                "Prepared range reader does not support filesystem '{kind}'"
            )))
        }
    };
    filesystem.map_err(FileServiceError::Io)
}
