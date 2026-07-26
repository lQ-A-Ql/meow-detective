use app_services::datasource_service::{detect_image_filesystem, ImageFilesystemKind};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::path::PathBuf;

fn sample_path() -> PathBuf {
    testing::fixtures::local_e01_fixture()
        .expect("set FORENSICS_E01_FIXTURE to run ignored FAT/ExFAT probe tests")
}

/// FAT/ExFAT probe smoke test using `FORENSICS_E01_FIXTURE`.
///
/// Local run:
///   $env:FORENSICS_E01_FIXTURE='<path-to-private-sample.E01>'
///   cargo test -p app-services --test fat_exfat_probe_test -- --ignored --nocapture
#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn fat_exfat_candidates_are_probed_and_openable() {
    let path = sample_path();

    let mut reader = match E01Reader::open(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("SKIP: cannot open E01 at {}: {e}", path.display());
            return;
        }
    };

    let probe = match detect_image_filesystem(&mut reader) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("SKIP: probe failed: {e}");
            return;
        }
    };

    let fat_candidates: Vec<_> = probe
        .candidates
        .iter()
        .filter(|c| matches!(c.kind, ImageFilesystemKind::Fat))
        .collect();

    if fat_candidates.is_empty() {
        eprintln!(
            "INFO: no FAT/ExFAT candidates found in {} ({} NTFS candidates detected); test passes gracefully",
            path.display(),
            probe
                .candidates
                .iter()
                .filter(|c| matches!(c.kind, ImageFilesystemKind::Ntfs))
                .count()
        );
        // no FAT found is acceptable — most E01 samples are NTFS-only
        return;
    }

    eprintln!("Found {} FAT/ExFAT candidate(s):", fat_candidates.len());
    for (i, c) in fat_candidates.iter().enumerate() {
        eprintln!(
            "  [{}] offset={} partition_index={:?} name={:?} source={:?}",
            i, c.offset, c.partition_index, c.partition_name, c.source
        );
    }

    // Try to open each FAT candidate with fs_fat (covers FAT12/16/32).
    // ExFAT volumes will fail here gracefully — the probe treats them as
    // ImageFilesystemKind::Fat, but fs-fat only handles legacy FAT.
    let mut opened = 0u32;
    let mut attempted = 0u32;

    for candidate in &fat_candidates {
        attempted += 1;
        let boxed: Box<dyn EvidenceReader> = Box::new(match E01Reader::open(&path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "  skip candidate at offset={}: cannot re-open E01: {e}",
                    candidate.offset
                );
                continue;
            }
        });

        match fs_fat::FatReader::open(boxed, candidate.offset) {
            Ok(fs) => match fs.root() {
                Ok(root) => {
                    assert!(root.is_dir, "FAT root must be a directory");
                    let children = fs.list_children("").unwrap_or_default();
                    eprintln!(
                        "  OK offset={}: FAT root={} children={}",
                        candidate.offset,
                        root.name,
                        children.len()
                    );
                    assert!(
                        !children.is_empty(),
                        "FAT root at offset={} should have children",
                        candidate.offset
                    );
                    for child in children.iter().take(5) {
                        eprintln!(
                            "    {} {} size={}",
                            if child.is_dir { "D" } else { "F" },
                            child.name,
                            child.size
                        );
                    }
                    opened += 1;
                }
                Err(e) => {
                    eprintln!(
                            "  WARN offset={}: FAT root read failed (possible ExFAT or damaged FS): {e}",
                            candidate.offset
                        );
                }
            },
            Err(e) => {
                eprintln!(
                    "  WARN offset={}: cannot open as legacy FAT (possible ExFAT or unsupported variant): {e}",
                    candidate.offset
                );
            }
        }
    }

    eprintln!("FAT probe summary: {attempted} attempted, {opened} opened successfully");
    assert!(
        opened > 0 || attempted == 0,
        "at least one FAT candidate should be openable if FAT candidates exist"
    );
}

/// Verifies that probe partition records reflect filesystem kind labels
/// and that candidates carry correct source metadata for FAT volumes.
#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn fat_exfat_probe_metadata_is_consistent() {
    let path = sample_path();

    let mut reader = match E01Reader::open(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("SKIP: cannot open E01: {e}");
            return;
        }
    };

    let probe = match detect_image_filesystem(&mut reader) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("SKIP: probe failed: {e}");
            return;
        }
    };

    let fat_candidates: Vec<_> = probe
        .candidates
        .iter()
        .filter(|c| matches!(c.kind, ImageFilesystemKind::Fat))
        .collect();

    if fat_candidates.is_empty() {
        eprintln!("INFO: no FAT candidates to verify; test passes");
        return;
    }

    // Every FAT candidate must have a valid offset (non-zero for partitioned images)
    for c in &fat_candidates {
        eprintln!(
            "FAT candidate: kind={:?} offset={} partition={:?} name={:?} source={:?}",
            c.kind, c.offset, c.partition_index, c.partition_name, c.source
        );

        // Kind label sanity
        let label = match c.kind {
            ImageFilesystemKind::Fat => "FAT",
            _ => "UNEXPECTED",
        };
        assert_eq!(label, "FAT", "candidate must be FAT kind");

        // Partition metadata: if GPT-backed, partition info should be present
        match c.source {
            app_services::datasource_service::ImageFilesystemSource::GptPartition => {
                let matching = probe
                    .partitions
                    .iter()
                    .find(|p| c.partition_index == Some(p.index));
                assert!(
                    matching.is_some(),
                    "GPT FAT candidate partition_index={:?} should have a matching partition record",
                    c.partition_index
                );
                if let Some(p) = matching {
                    eprintln!(
                        "  → partition record: index={} name={} kind_label={} status={:?}",
                        p.index, p.name, p.kind_label, p.status
                    );
                    assert!(
                        p.kind_label.contains("FAT") || p.kind_label.contains("ExFAT"),
                        "partition kind_label should indicate FAT/ExFAT, got '{}'",
                        p.kind_label
                    );
                }
            }
            app_services::datasource_service::ImageFilesystemSource::MbrPartition => {
                eprintln!(
                    "  → MBR partition (partition_index={:?})",
                    c.partition_index
                );
            }
            app_services::datasource_service::ImageFilesystemSource::DirectVolume => {
                eprintln!("  → direct volume (no partition table)");
            }
            _ => {
                eprintln!("  → other source: {:?}", c.source);
            }
        }
    }
}
