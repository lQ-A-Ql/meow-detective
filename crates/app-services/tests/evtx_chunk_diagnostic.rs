//! Opt-in, read-only EVTX chunk and NTFS extent diagnosis for private images.

use app_services::datasource_service::{detect_image_filesystem, ImageFilesystemKind};
use evidence_core::FileSystemReader;
use fs_ntfs::NtfsReader;
use image_e01::E01Reader;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const SYSTEM_PATH: &str = "Windows/System32/winevt/Logs/System.evtx";
const HEADER_SIZE: usize = 4096;
const CHUNK_SIZE: usize = 65536;

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE private E01 sample"]
fn diagnose_system_evtx_chunks_and_ntfs_sizes() {
    let fixture = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE");
    let (fs, volume_offset) = windows_filesystem(&fixture);
    let inode = fs
        .preview_file(SYSTEM_PATH)
        .expect("resolve System.evtx")
        .inode();
    let technical = fs
        .inspect_file_by_inode(inode)
        .expect("inspect System.evtx MFT");
    let mut bytes = Vec::new();
    fs.open_file(SYSTEM_PATH)
        .expect("open System.evtx")
        .read_to_end(&mut bytes)
        .expect("read System.evtx");
    let attributes = technical
        .attributes
        .iter()
        .map(|attribute| {
            let runs = attribute
                .data_runs
                .iter()
                .map(|run| {
                    json!({
                        "logicalOffset": run.logical_offset,
                        "physicalOffset": run.physical_offset,
                        "clusterCount": run.cluster_count,
                        "absoluteLcn": run.absolute_lcn,
                        "raw": hex::encode(&run.raw),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "type": attribute.attribute_type,
                "name": attribute.name,
                "nonResident": attribute.non_resident,
                "allocatedSize": attribute.allocated_size,
                "realSize": attribute.real_size,
                "initializedSize": attribute.initialized_size,
                "runs": runs,
            })
        })
        .collect::<Vec<_>>();
    let extraction = artifacts_windows::extract_structured_events(&bytes, SYSTEM_PATH)
        .expect("parse System.evtx");
    verify_logical_read_paths(&fixture, &fs, volume_offset, inode, &bytes, &extraction);
    let manifest = json!({
        "volumeOffset": volume_offset,
        "clusterSize": fs.cluster_size(),
        "inode": inode,
        "mftRecordOffset": technical.record_offset,
        "mftRecordSize": technical.record_size,
        "readLength": bytes.len(),
        "sha256": hex::encode(Sha256::digest(&bytes)),
        "firstChunk": u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        "lastChunk": u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        "nextRecord": u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        "declaredChunks": u16::from_le_bytes(bytes[42..44].try_into().unwrap()),
        "flags": u32::from_le_bytes(bytes[120..124].try_into().unwrap()),
        "attributes": attributes,
        "chunks": chunk_diagnostics(&bytes),
        "bootEvents": extraction.boot_events.len(),
        "securityEvents": extraction.security_events.len(),
        "applicationEvents": extraction.application_events.len(),
        "warnings": extraction.warnings,
        "logicalReadSimulation": logical_read_simulation(&bytes, &technical),
        "logicalReadPathsAgree": true,
        "baselineComparison": compare_before_patch(&bytes, &technical, &extraction),
    });
    eprintln!("{}", serde_json::to_string_pretty(&manifest).unwrap());
    if let Some(output) = std::env::var_os("FORENSICS_EVTX_DIAGNOSTIC_OUTPUT") {
        let output = PathBuf::from(output);
        std::fs::create_dir_all(&output).expect("create diagnostic output directory");
        write_new(&output.join("System.evtx"), &bytes);
        write_new(&output.join("System.mft-record.bin"), &technical.record_raw);
        write_new(
            &output.join("diagnosis.json"),
            &serde_json::to_vec_pretty(&manifest).unwrap(),
        );
    }
}

fn verify_logical_read_paths(
    fixture: &Path,
    filesystem: &NtfsReader,
    volume_offset: u64,
    inode: u64,
    bytes: &[u8],
    extraction: &artifacts_windows::EvtxStructuredExtraction,
) {
    assert_eq!(
        filesystem
            .read_file_range_by_inode(inode, 0, bytes.len())
            .unwrap(),
        bytes
    );
    let mut stream = NtfsReader::open(Box::new(E01Reader::open(fixture).unwrap()), volume_offset)
        .unwrap()
        .into_file_stream_by_inode(inode)
        .unwrap();
    let mut streamed_bytes = Vec::new();
    stream.read_to_end(&mut streamed_bytes).unwrap();
    assert_eq!(streamed_bytes, bytes);
    stream.seek(SeekFrom::Start(0)).unwrap();
    let mut streamed = artifacts_windows::EvtxStructuredExtraction::default();
    let summary =
        artifacts_windows::visit_structured_events_from_read_seek(stream, SYSTEM_PATH, |event| {
            match event {
                artifacts_windows::EvtxStructuredEvent::Boot(event) => {
                    streamed.boot_events.push(event)
                }
                artifacts_windows::EvtxStructuredEvent::Security(event) => {
                    streamed.security_events.push(event)
                }
                artifacts_windows::EvtxStructuredEvent::Application(event) => {
                    streamed.application_events.push(event)
                }
            }
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap();
    streamed.warnings = summary.warnings;
    assert_eq!(&streamed, extraction);
}

fn compare_before_patch(
    bytes: &[u8],
    technical: &fs_ntfs::NtfsTechnicalFile,
    extraction: &artifacts_windows::EvtxStructuredExtraction,
) -> Option<serde_json::Value> {
    let baseline = std::env::var_os("FORENSICS_EVTX_DIAGNOSTIC_BASELINE")?;
    let before = std::fs::read(baseline).expect("read explicit pre-patch EVTX baseline");
    let initialized = technical
        .attributes
        .iter()
        .find(|attribute| attribute.attribute_type == 0x80 && attribute.name.is_none())
        .and_then(|attribute| attribute.initialized_size)
        .unwrap() as usize;
    let mut expected = before.clone();
    expected[initialized..].fill(0);
    assert_eq!(bytes, expected);
    let previous = artifacts_windows::extract_structured_events(&before, SYSTEM_PATH).unwrap();
    assert_eq!(extraction.boot_events, previous.boot_events);
    assert_eq!(extraction.security_events, previous.security_events);
    assert_eq!(extraction.application_events, previous.application_events);
    assert!(extraction.warnings.is_empty());
    Some(json!({
        "sameInitializedPrefix": true,
        "sameSupportedEvents": true,
        "beforeWarnings": previous.warnings.len(),
        "afterWarnings": extraction.warnings.len(),
        "baselineSha256": hex::encode(Sha256::digest(&before)),
    }))
}

fn logical_read_simulation(
    bytes: &[u8],
    technical: &fs_ntfs::NtfsTechnicalFile,
) -> serde_json::Value {
    let initialized = technical
        .attributes
        .iter()
        .find(|attribute| attribute.attribute_type == 0x80 && attribute.name.is_none())
        .and_then(|attribute| attribute.initialized_size)
        .expect("System.evtx has a non-resident unnamed data attribute")
        as usize;
    let mut logical_bytes = bytes.to_vec();
    logical_bytes[initialized.min(bytes.len())..].fill(0);
    let original = artifacts_windows::extract_structured_events(bytes, SYSTEM_PATH).unwrap();
    let logical =
        artifacts_windows::extract_structured_events(&logical_bytes, SYSTEM_PATH).unwrap();
    assert_eq!(original.boot_events, logical.boot_events);
    assert_eq!(original.security_events, logical.security_events);
    assert_eq!(original.application_events, logical.application_events);
    json!({
        "initializedSize": initialized,
        "zeroFilledBytes": bytes.len().saturating_sub(initialized),
        "sameSupportedEvents": true,
        "bootEvents": logical.boot_events.len(),
        "warnings": logical.warnings,
        "sha256": hex::encode(Sha256::digest(&logical_bytes)),
    })
}

fn windows_filesystem(fixture: &Path) -> (NtfsReader, u64) {
    let mut image = E01Reader::open(fixture).expect("open E01");
    let probe = detect_image_filesystem(&mut image).expect("probe E01");
    for candidate in probe.candidates {
        if !matches!(candidate.kind, ImageFilesystemKind::Ntfs) {
            continue;
        }
        let fs = NtfsReader::open(
            Box::new(E01Reader::open(fixture).expect("reopen E01")),
            candidate.offset,
        )
        .expect("open NTFS candidate");
        if fs.preview_file(SYSTEM_PATH).is_ok() {
            return (fs, candidate.offset);
        }
    }
    panic!("fixture does not contain System.evtx on an NTFS volume");
}

fn chunk_diagnostics(bytes: &[u8]) -> Vec<serde_json::Value> {
    bytes[HEADER_SIZE..]
        .chunks_exact(CHUNK_SIZE)
        .enumerate()
        .map(|(id, chunk)| {
            let valid_magic = chunk.starts_with(b"ElfChnk\0");
            let mut diagnostic = json!({
                "id": id,
                "offset": HEADER_SIZE + id * CHUNK_SIZE,
                "magicHex": hex::encode(&chunk[..8]),
                "validMagic": valid_magic,
                "allZero": chunk.iter().all(|byte| *byte == 0),
                "sha256": hex::encode(Sha256::digest(chunk)),
            });
            if valid_magic {
                diagnostic["firstRecordId"] =
                    json!(u64::from_le_bytes(chunk[24..32].try_into().unwrap()));
                diagnostic["lastRecordId"] =
                    json!(u64::from_le_bytes(chunk[32..40].try_into().unwrap()));
                diagnostic["freeSpaceOffset"] =
                    json!(u32::from_le_bytes(chunk[48..52].try_into().unwrap()));
            }
            diagnostic
        })
        .collect()
}

fn write_new(path: &Path, bytes: &[u8]) {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("diagnostic destination must not already exist")
        .write_all(bytes)
        .expect("write diagnostic output");
}
