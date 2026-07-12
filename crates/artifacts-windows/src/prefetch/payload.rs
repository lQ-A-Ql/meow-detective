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
    use windows_sys::Win32::Storage::Compression::{
        COMPRESS_ALGORITHM_LZMS, COMPRESS_ALGORITHM_XPRESS, COMPRESS_ALGORITHM_XPRESS_HUFF,
        COMPRESS_RAW,
    };

    if data.len() < 10 {
        return Err("Compressed Prefetch payload is truncated".to_string());
    }
    let expected_size = u32::from_le_bytes(data[4..8].try_into().unwrap_or_default()) as usize;
    if expected_size == 0 || expected_size > 128 * 1024 * 1024 {
        return Err("Compressed Prefetch declares an invalid uncompressed size".to_string());
    }
    let algorithms = [
        COMPRESS_ALGORITHM_XPRESS_HUFF | COMPRESS_RAW,
        COMPRESS_ALGORITHM_XPRESS | COMPRESS_RAW,
        COMPRESS_ALGORITHM_LZMS | COMPRESS_RAW,
        COMPRESS_ALGORITHM_XPRESS_HUFF,
        COMPRESS_ALGORITHM_XPRESS,
        COMPRESS_ALGORITHM_LZMS,
    ];
    let mut last_error = None;
    for algorithm in algorithms {
        match try_decompress(algorithm, &data[8..], expected_size) {
            Ok(decoded) if decoded.get(4..8) == Some(UNCOMPRESSED_SIGNATURE.as_slice()) => {
                return Ok(decoded);
            }
            Ok(_) => {
                last_error =
                    Some("Compressed Prefetch decompressed without an SCCA payload".to_string())
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        "Compressed Prefetch could not be decompressed with supported algorithms".to_string()
    }))
}

#[cfg(not(windows))]
fn decode_mam_prefetch(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("MAM-compressed Prefetch requires Windows decompression support".to_string())
}

#[cfg(windows)]
fn try_decompress(algorithm: u32, data: &[u8], expected_size: usize) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Storage::Compression::{
        CreateDecompressor, Decompress, DECOMPRESSOR_HANDLE,
    };

    let mut handle: DECOMPRESSOR_HANDLE = std::ptr::null_mut();
    // SAFETY: the API writes a new decompressor handle to the valid output pointer.
    let created = unsafe { CreateDecompressor(algorithm, std::ptr::null(), &mut handle) };
    if created == 0 || handle.is_null() {
        return Err(last_os_error("CreateDecompressor failed"));
    }
    let _guard = DecompressorGuard(handle);
    let mut decoded = vec![0u8; expected_size];
    let mut actual_size = 0usize;
    // SAFETY: input and output buffers are valid for the supplied lengths.
    let ok = unsafe {
        Decompress(
            handle,
            data.as_ptr().cast(),
            data.len(),
            decoded.as_mut_ptr().cast(),
            decoded.len(),
            &mut actual_size,
        )
    };
    if ok == 0 {
        return Err(last_os_error("Decompress failed"));
    }
    decoded.truncate(actual_size);
    Ok(decoded)
}

#[cfg(windows)]
struct DecompressorGuard(windows_sys::Win32::Storage::Compression::DECOMPRESSOR_HANDLE);

#[cfg(windows)]
impl Drop for DecompressorGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle was created by CreateDecompressor and is closed once.
            unsafe {
                windows_sys::Win32::Storage::Compression::CloseDecompressor(self.0);
            }
        }
    }
}

#[cfg(windows)]
fn last_os_error(prefix: &str) -> String {
    format!("{prefix}: {}", std::io::Error::last_os_error())
}
