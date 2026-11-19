//! Read-only XFS reflink layout probe over the private E01 fixture.
//!
//! Opens the evidence through `evidence_block`, enumerates partition spans with
//! the existing `source_kernel::partition_probe` detector, then opens every
//! span with `fs_xfs::XfsReader`. For each XFS volume it prints the volume
//! reflink capability, the read-only `assess_in_place_file_rewrite` verdict for
//! `etc/shadow`, the shadow/passwd sizes, and nothing is ever written.
//!
//! ```text
//! cargo test -p app-services --test xfs_reflink_probe -- --include-ignored --nocapture
//! ```

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;

use evidence_block::{open_block_provider, BlockDeviceError, BlockProvider, EvidenceImageKind};
use evidence_core::{EvidenceReader, PartitionWindowReader, ReaderInfo};
use fs_xfs::XfsReader;

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_XFS_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_XFS_E01_FIXTURE")
}

fn block_error(error: BlockDeviceError) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

/// Thread-free random-access adapter over an `Arc<dyn BlockProvider>`. Each
/// instance owns its own seek cursor; the provider itself stays read-only.
struct ProviderReader {
    provider: Arc<dyn BlockProvider>,
    position: u64,
    info: ReaderInfo,
}

impl ProviderReader {
    fn new(provider: Arc<dyn BlockProvider>, path: PathBuf) -> Self {
        let size = provider.len();
        Self {
            provider,
            position: 0,
            info: ReaderInfo {
                path,
                size,
                kind: "block-provider-adapter".to_string(),
            },
        }
    }
}

impl Read for ProviderReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.provider.len().saturating_sub(self.position);
        let want = buf.len().min(remaining as usize);
        if want == 0 {
            return Ok(0);
        }
        self.provider
            .read_exact_at(self.position, &mut buf[..want])
            .map_err(block_error)?;
        self.position += want as u64;
        Ok(want)
    }
}

impl Seek for ProviderReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.provider.len()) + i128::from(delta),
        };
        if next < 0 || next > i128::from(self.provider.len()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek outside evidence bounds",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

impl EvidenceReader for ProviderReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

#[test]
#[ignore = "requires the private XFS E01 sample"]
fn probe_xfs_reflink_layout() {
    let image = sample_path();
    let provider = open_block_provider(&image, EvidenceImageKind::E01)
        .expect("open the E01 block provider read-only");
    eprintln!(
        "image: {} ({} bytes, provider_len={})",
        image.display(),
        std::fs::metadata(&image).unwrap().len(),
        provider.len()
    );

    // Enumerate candidate partition spans with the existing probe. The probe
    // covers ISO9660, direct volumes, MBR primary/logical entries and GPT
    // partitions; a failure degrades to a whole-image fallback below.
    let mut spans: Vec<(u64, Option<u64>)> = Vec::new();
    let mut adapter = ProviderReader::new(Arc::clone(&provider), image.clone());
    match source_kernel::partition_probe::detect_image_filesystem(&mut adapter) {
        Ok(probe) => {
            eprintln!(
                "probe: {} partition record(s), {} candidate(s), warnings={:?}",
                probe.partitions.len(),
                probe.candidates.len(),
                probe.warnings
            );
            for record in &probe.partitions {
                spans.push((record.offset, (record.length > 0).then_some(record.length)));
            }
            for candidate in &probe.candidates {
                spans.push((candidate.offset, candidate.length));
            }
        }
        Err(error) => {
            eprintln!("detect_image_filesystem failed; falling back to whole image: {error}")
        }
    }
    // Always try the whole image itself (filesystem directly on the image).
    spans.push((0, None));
    spans.sort_by_key(|(offset, _)| *offset);
    spans.dedup_by_key(|(offset, _)| *offset);

    let mut xfs_count = 0usize;
    for (offset, length) in spans {
        eprintln!("--- candidate span offset={offset} length={length:?} ---");
        let reader = ProviderReader::new(Arc::clone(&provider), image.clone());
        let window = match PartitionWindowReader::new(Box::new(reader), offset, length) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("window rejected: {error}");
                continue;
            }
        };
        let fs = match XfsReader::open(Box::new(window), 0) {
            Ok(fs) => fs,
            Err(error) => {
                eprintln!("not XFS at this span: {error}");
                continue;
            }
        };
        xfs_count += 1;
        eprintln!("=== XFS volume at offset {offset} (length {length:?}) ===");
        eprintln!("volume_reflink_enabled = {}", fs.volume_reflink_enabled());

        let assessment = fs.assess_in_place_file_rewrite("etc/shadow", b"probe");
        eprintln!("assess_in_place_file_rewrite(etc/shadow) = {assessment:?}");
        eprintln!(
            "  target_inode_reflink = {:?}, log_clean = {}, supported = {}, blocker = {:?}",
            assessment.target_inode_reflink,
            assessment.log_clean,
            assessment.supported,
            assessment.blocker
        );

        match fs.file_size_by_path("etc/shadow") {
            Ok(size) => eprintln!("etc/shadow readable, size = {size} bytes"),
            Err(error) => eprintln!("etc/shadow size lookup failed: {error}"),
        }
        match fs.file_size_by_path("etc/passwd") {
            Ok(size) => eprintln!("etc/passwd readable, size = {size} bytes"),
            Err(error) => eprintln!("etc/passwd lookup failed: {error}"),
        }
    }

    assert!(
        xfs_count > 0,
        "at least one XFS partition must open from the fixture"
    );
}
