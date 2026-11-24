//! Bounded LZNT1 decoding through the Windows compression implementation.

use std::io;

#[cfg(windows)]
pub(crate) fn decode_lznt1(input: &[u8], capacity: usize) -> io::Result<Vec<u8>> {
    use windows_sys::Wdk::Storage::FileSystem::RtlDecompressBuffer;
    use windows_sys::Win32::Storage::FileSystem::COMPRESSION_FORMAT_LZNT1;

    let mut output = vec![0; capacity];
    let mut written = 0;
    // SAFETY: both buffers remain valid for their checked lengths and the API
    // writes at most output.len() bytes. written points to a valid u32.
    let status = unsafe {
        RtlDecompressBuffer(
            COMPRESSION_FORMAT_LZNT1,
            output.as_mut_ptr(),
            u32::try_from(capacity)
                .map_err(|_| crate::invalid_fs_data("LZNT1 capacity exceeds u32"))?,
            input.as_ptr(),
            u32::try_from(input.len())
                .map_err(|_| crate::invalid_fs_data("LZNT1 input exceeds u32"))?,
            &mut written,
        )
    };
    if status != 0 {
        return Err(crate::invalid_fs_data(format!(
            "LZNT1 decompression failed with NTSTATUS {status:#010x}"
        )));
    }
    if written as usize > capacity {
        return Err(crate::invalid_fs_data("LZNT1 output exceeds unit capacity"));
    }
    output.truncate(written as usize);
    Ok(output)
}

#[cfg(not(windows))]
pub(crate) fn decode_lznt1(_input: &[u8], _capacity: usize) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "LZNT1 requires the Windows compression provider",
    ))
}
