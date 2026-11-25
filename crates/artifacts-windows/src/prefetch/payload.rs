const UNCOMPRESSED_SIGNATURE: &[u8; 4] = b"SCCA";
const MAM_SIGNATURE: &[u8; 4] = b"MAM\x04";

pub(super) fn decode_prefetch_payload(data: Vec<u8>) -> Result<Vec<u8>, String> {
    if data.get(4..8) == Some(UNCOMPRESSED_SIGNATURE.as_slice()) {
        return Ok(data);
    }
    if data.starts_with(MAM_SIGNATURE) {
        return decode_mam_prefetch(&data);
    }
    Err("Not a Prefetch file".to_string())
}

#[cfg(windows)]
fn decode_mam_prefetch(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Wdk::Storage::FileSystem::{
        RtlDecompressBufferEx, RtlGetCompressionWorkSpaceSize,
    };
    use windows_sys::Win32::Storage::FileSystem::COMPRESSION_FORMAT_XPRESS_HUFF;

    if data.len() < 10 {
        return Err("Compressed Prefetch payload is truncated".into());
    }
    let expected = u32::from_le_bytes(data[4..8].try_into().expect("validated MAM header"));
    if expected == 0 || expected > 128 * 1024 * 1024 {
        return Err("Compressed Prefetch declares an invalid output size".into());
    }
    let mut buffer_workspace = 0;
    let mut fragment_workspace = 0;
    // SAFETY: both output arguments reference initialized u32 variables.
    let status = unsafe {
        RtlGetCompressionWorkSpaceSize(
            COMPRESSION_FORMAT_XPRESS_HUFF,
            &mut buffer_workspace,
            &mut fragment_workspace,
        )
    };
    if status != 0 {
        return Err(format!(
            "XPRESS Huffman workspace failed: NTSTATUS {status:#010x}"
        ));
    }
    if fragment_workspace > 16 * 1024 * 1024 {
        return Err("XPRESS Huffman workspace exceeds 16 MiB".into());
    }
    let mut workspace = vec![0u8; fragment_workspace as usize];
    let mut decoded = vec![0u8; expected as usize];
    let mut written = 0;
    // SAFETY: input and output lengths match their buffers. The workspace size
    // is supplied by the compression provider for fragment decompression.
    let status = unsafe {
        RtlDecompressBufferEx(
            COMPRESSION_FORMAT_XPRESS_HUFF,
            decoded.as_mut_ptr(),
            expected,
            data[8..].as_ptr(),
            u32::try_from(data.len() - 8).map_err(|error| error.to_string())?,
            &mut written,
            workspace.as_mut_ptr().cast(),
        )
    };
    if status != 0 {
        return Err(format!(
            "XPRESS Huffman decompression failed: NTSTATUS {status:#010x}"
        ));
    }
    if written != expected || decoded.get(4..8) != Some(UNCOMPRESSED_SIGNATURE.as_slice()) {
        return Err("Prefetch decoded size or SCCA signature is invalid".into());
    }
    Ok(decoded)
}

#[cfg(not(windows))]
fn decode_mam_prefetch(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("MAM-compressed Prefetch requires the Windows compression provider".into())
}
