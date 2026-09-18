//! Real E01 probe regressions.
//!
//! These tests validate the probe contract instead of assuming every image is
//! a Windows NTFS disk. Linux GPT images may contain Ext4, XFS, and LVM
//! candidates alongside unsupported metadata partitions.
//!
//! Run with:
//!   $env:FORENSICS_E01_FIXTURE='<path-to-private-sample.E01>'
//!   cargo test -p app-services --test e01_probe_real_test -- --ignored --nocapture

use app_services::datasource_service::{detect_image_filesystem, ImageFilesystemKind};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

fn sample_path() -> PathBuf {
    testing::fixtures::local_e01_fixture()
        .unwrap_or_else(|| panic!("set FORENSICS_E01_FIXTURE to run ignored real E01 tests"))
}

fn open_candidate(
    path: &Path,
    kind: ImageFilesystemKind,
    offset: u64,
) -> Result<Option<Box<dyn FileSystemReader>>, String> {
    let reader = || E01Reader::open(path).map(|reader| Box::new(reader) as Box<dyn EvidenceReader>);
    match kind {
        ImageFilesystemKind::Ntfs => {
            fs_ntfs::NtfsReader::open(reader().map_err(|e| e.to_string())?, offset)
                .map(|fs| Some(Box::new(fs) as Box<dyn FileSystemReader>))
                .map_err(|e| e.to_string())
        }
        ImageFilesystemKind::Fat => {
            fs_fat::FatReader::open(reader().map_err(|e| e.to_string())?, offset)
                .map(|fs| Some(Box::new(fs) as Box<dyn FileSystemReader>))
                .map_err(|e| e.to_string())
        }
        ImageFilesystemKind::Ext4 => {
            fs_ext4::Ext4Reader::open(reader().map_err(|e| e.to_string())?, offset)
                .map(|fs| Some(Box::new(fs) as Box<dyn FileSystemReader>))
                .map_err(|e| e.to_string())
        }
        ImageFilesystemKind::F2fs => {
            fs_f2fs::F2fsReader::open(reader().map_err(|e| e.to_string())?, offset)
                .map(|fs| Some(Box::new(fs) as Box<dyn FileSystemReader>))
                .map_err(|e| e.to_string())
        }
        ImageFilesystemKind::Erofs => {
            fs_erofs::ErofsReader::open(reader().map_err(|e| e.to_string())?, offset)
                .map(|fs| Some(Box::new(fs) as Box<dyn FileSystemReader>))
                .map_err(|e| e.to_string())
        }
        ImageFilesystemKind::Xfs => {
            fs_xfs::XfsReader::open(reader().map_err(|e| e.to_string())?, offset)
                .map(|fs| Some(Box::new(fs) as Box<dyn FileSystemReader>))
                .map_err(|e| e.to_string())
        }
        ImageFilesystemKind::Btrfs => {
            fs_btrfs::BtrfsReader::open(reader().map_err(|e| e.to_string())?, offset)
                .map(|fs| Some(Box::new(fs) as Box<dyn FileSystemReader>))
                .map_err(|e| e.to_string())
        }
        ImageFilesystemKind::Iso9660 => evidence_core::Iso9660Reader::from_reader(
            reader().map_err(|e| e.to_string())?,
            path.file_name().and_then(|name| name.to_str()),
        )
        .map(|fs| Some(Box::new(fs) as Box<dyn FileSystemReader>))
        .map_err(|e| e.to_string()),
        ImageFilesystemKind::BitLocker | ImageFilesystemKind::LvmPool => Ok(None),
    }
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn real_e01_probe_reports_candidates_without_format_assumptions() {
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();

    assert!(
        !probe.partitions.is_empty(),
        "E01 should expose partition metadata"
    );
    assert!(
        !probe.candidates.is_empty(),
        "E01 should expose readable candidates"
    );

    for candidate in &probe.candidates {
        assert!(
            candidate.offset % 512 == 0,
            "candidate offset must be sector aligned"
        );
        assert!(
            candidate.partition_index.is_some(),
            "partition-backed candidates must retain their partition index"
        );
        assert!(
            probe
                .partitions
                .iter()
                .any(|partition| Some(partition.index) == candidate.partition_index),
            "candidate must refer to a probed partition: {candidate:?}"
        );
    }
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn real_e01_opens_each_readable_filesystem_candidate() {
    let path = sample_path();
    let mut reader = E01Reader::open(&path).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let mut opened = 0usize;

    for candidate in &probe.candidates {
        let Some(fs) =
            open_candidate(&path, candidate.kind, candidate.offset).unwrap_or_else(|error| {
                panic!(
                    "failed to open {:?} candidate at {}: {error}",
                    candidate.kind, candidate.offset
                )
            })
        else {
            continue;
        };
        let root = fs.root().unwrap_or_else(|error| {
            panic!(
                "failed to read {:?} root at {}: {error}",
                candidate.kind, candidate.offset
            )
        });
        assert!(root.is_dir, "filesystem root must be a directory: {root:?}");
        opened += 1;
    }

    assert!(
        opened > 0,
        "the E01 probe returned no directly readable filesystem candidate"
    );
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn real_e01_cross_chunk_read_remains_independent_from_probe() {
    let path = sample_path();
    let mut reader = E01Reader::open(&path).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    assert!(!probe.candidates.is_empty());

    let chunk_size = reader.preferred_read_granularity();
    assert!(
        chunk_size >= 512,
        "E01 chunk must contain at least one sector"
    );
    assert!(
        reader.info().size > chunk_size as u64,
        "real sample must contain more than one E01 chunk"
    );

    let cross_offset = chunk_size - 256;
    reader.seek(SeekFrom::Start(cross_offset as u64)).unwrap();
    let mut cross_chunk = [0u8; 512];
    reader.read_exact(&mut cross_chunk).unwrap();

    reader.seek(SeekFrom::Start(cross_offset as u64)).unwrap();
    let mut repeated = [0u8; 512];
    reader.read_exact(&mut repeated).unwrap();
    assert_eq!(cross_chunk, repeated);
}
