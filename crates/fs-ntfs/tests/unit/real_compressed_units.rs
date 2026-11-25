#![cfg(windows)]

use super::*;
use serde_json::json;

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE, FORENSICS_NTFS_VOLUME_OFFSET, FORENSICS_COMPRESSED_RECORD_IDS and FORENSICS_CODEC_REPORT"]
fn real_compressed_units_are_checked_by_independent_decoders() {
    let fixture = std::env::var("FORENSICS_E01_FIXTURE").expect("set FORENSICS_E01_FIXTURE");
    let offset = std::env::var("FORENSICS_NTFS_VOLUME_OFFSET")
        .unwrap()
        .parse()
        .unwrap();
    let ids: Vec<u64> = serde_json::from_slice(
        &std::fs::read(std::env::var("FORENSICS_COMPRESSED_RECORD_IDS").unwrap()).unwrap(),
    )
    .unwrap();
    assert!(!ids.is_empty() && ids.len() <= 64);
    let before = std::fs::metadata(&fixture).unwrap();
    let report = std::path::PathBuf::from(std::env::var("FORENSICS_CODEC_REPORT").unwrap());
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    assert!(report
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .starts_with(workspace));
    let fs = crate::NtfsReader::open(
        Box::new(image_e01::E01Reader::open(std::path::Path::new(&fixture)).unwrap()),
        offset,
    )
    .unwrap();
    let mut observations = Vec::new();
    let mut verified_damage = 0;
    for inode in ids {
        let record = fs.read_mft_record(inode).unwrap();
        let extents = fs
            .collect_unnamed_data_extents_from_base(inode, &record)
            .unwrap();
        let initialized = data_extents_initialized_size(&extents, fs.cluster_size).unwrap();
        let layout = compression_runs(&extents, fs.cluster_size).unwrap();
        let (physical, encoded) = fs.read_physical_compression_unit(&layout, 0).unwrap();
        assert!(encoded);
        let raw_path = report.with_file_name(format!("unit-{inode}.bin"));
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&raw_path)
            .unwrap()
            .write_all(&physical)
            .unwrap();
        let native = decode_lznt1(&physical, layout.unit_bytes as usize);
        let portable =
            crate::compression::decode_lznt1_portable(&physical, layout.unit_bytes as usize);
        let required = initialized.min(layout.unit_bytes) as usize;
        let invalid = |result: &std::io::Result<Vec<u8>>| {
            !result.as_ref().is_ok_and(|bytes| bytes.len() >= required)
        };
        let damaged = invalid(&native) && invalid(&portable);
        verified_damage += usize::from(damaged);
        observations.push(json!({
            "inode": inode,
            "requiredBytes": required,
            "physicalBytes": physical.len(),
            "physicalUnitFile": raw_path,
            "bothProvidersConfirmInvalidUnit": damaged,
            "native": match native {
                Ok(bytes) => json!({"decodedBytes": bytes.len()}),
                Err(error) => json!({"error": error.to_string()}),
            },
            "portable": match portable {
                Ok(bytes) => json!({"decodedBytes": bytes.len()}),
                Err(error) => json!({"error": error.to_string()}),
            },
        }));
    }
    let after = std::fs::metadata(&fixture).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    assert_eq!(
        before.permissions().readonly(),
        after.permissions().readonly()
    );
    use std::io::Write;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(report)
        .unwrap();
    serde_json::to_writer_pretty(
        &mut output,
        &json!({"volumeOffset": offset, "observations": observations, "verifiedDamage": verified_damage,
            "inputMetadataUnchanged": true}),
    )
    .unwrap();
    output.flush().unwrap();
    assert_eq!(
        verified_damage,
        observations.len(),
        "independent decoders disagree on input validity"
    );
}
