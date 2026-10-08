use crate::wof::{codec, format::WofAlgorithm, xpress};

#[path = "../../support/wof_frames.rs"]
mod frames;

fn repeated_xpress(size: u16) -> Vec<u8> {
    let mut data = vec![0; 256];
    data[32] = 0x10; // 'A': canonical code 0.
    data[135] = 0x10; // Match with extended length and distance 1: code 1.
    data.extend_from_slice(&[0x00, 0x40, 0, 0, 255]);
    data.extend_from_slice(&(size - 4).to_le_bytes());
    data
}

#[test]
fn xpress_decodes_long_overlapping_matches_for_all_chunk_sizes() {
    for (algorithm, size) in [
        (WofAlgorithm::Xpress4k, 4096),
        (WofAlgorithm::Xpress8k, 8192),
        (WofAlgorithm::Xpress16k, 16384),
    ] {
        assert_eq!(
            codec::decompress(&repeated_xpress(size), size as usize, algorithm).unwrap(),
            vec![b'A'; size as usize]
        );
    }
}

#[test]
fn malformed_huffman_and_excessive_lengths_fail_without_large_allocations() {
    let mut data = repeated_xpress(4096);
    data[261..263].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(xpress::decompress(&data, 4096).is_err());
    assert!(xpress::decompress(&[0xff; 260], 4096).is_err());
    assert!(xpress::decompress(&[0; 258], 4096).is_err());
    let mut missing_history = repeated_xpress(4096);
    missing_history[257] = 0x80;
    assert!(xpress::decompress(&missing_history, 4096).is_err());
    let mut huge_length = repeated_xpress(4096);
    huge_length[261..263].fill(0);
    huge_length.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(xpress::decompress(&huge_length, 4096).is_err());
}

#[test]
fn reparse_versions_flags_and_provider_are_validated() {
    let mut data = Vec::new();
    data.extend_from_slice(&0x80000017u32.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    for value in [1u32, 2, 1, 0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    assert!(WofAlgorithm::from_reparse(&data).unwrap().is_some());
    let mut unknown = data.clone();
    unknown[12..16].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        WofAlgorithm::from_reparse(&unknown).unwrap_err().kind(),
        std::io::ErrorKind::Unsupported
    );
    unknown = data.clone();
    unknown[16..20].copy_from_slice(&2u32.to_le_bytes());
    assert!(WofAlgorithm::from_reparse(&unknown).is_err());
    unknown = data.clone();
    unknown[4..6].copy_from_slice(&20u16.to_le_bytes());
    unknown.extend_from_slice(&0u32.to_le_bytes());
    assert!(WofAlgorithm::from_reparse(&unknown).unwrap().is_some());
    unknown[24] = 1;
    assert!(WofAlgorithm::from_reparse(&unknown).is_err());
    unknown = data;
    unknown[6] = 1;
    assert!(WofAlgorithm::from_reparse(&unknown).is_err());
}

#[test]
fn lzx_decodes_verbatim_aligned_raw_blocks_and_e8_transforms() {
    for aligned in [false, true] {
        for prefix in [&[][..], &b"MZ\0\0\xE8\x10\0\0\0abcd"[..]] {
            let (compressed, expected) = frames::lzx(32768, aligned, prefix);
            assert_eq!(
                codec::decompress(&compressed, expected.len(), WofAlgorithm::Lzx).unwrap(),
                expected
            );
            if let Some(root) = std::env::var_os("FORENSICS_WOF_CODEC_EXPORT_DIR") {
                let root = tempfile::Builder::new()
                    .prefix("lzx-vector-")
                    .tempdir_in(root)
                    .unwrap()
                    .keep();
                std::fs::write(root.join("compressed.bin"), &compressed).unwrap();
                std::fs::write(root.join("expected.bin"), &expected).unwrap();
                println!("LZX_ORACLE_VECTOR={}", root.display());
            }
        }
    }
    assert_eq!(
        codec::decompress(&frames::xpress(4096), 4096, WofAlgorithm::Xpress4k).unwrap(),
        vec![b'A'; 4096]
    );
}
