use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
};
use transport::dto::bolt_reader::{BoltReadRequestDto, BOLT_READER_PROTOCOL_VERSION};

pub(super) fn request() -> Result<BoltReadRequestDto, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 128 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "request limit exceeded").into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) fn validate(request: &BoltReadRequestDto) -> io::Result<()> {
    let limits = &request.limits;
    let invalid = request.protocol_version != BOLT_READER_PROTOCOL_VERSION
        || request.request_id.is_empty()
        || request.request_id.len() > 128
        || request.input_path.len() > 64 * 1024
        || request.input_sha256.len() != 64
        || request.prefix.len() > 32
        || request.parent_suffix.len() > 32
        || request.record_keys.len() > 64
        || request.value_keys.len() > 64
        || request.input_size > limits.max_file_bytes
        || limits.max_file_bytes > 512 * 1024 * 1024
        || limits.max_entries == 0
        || limits.max_entries > 1_048_576
        || limits.max_depth == 0
        || limits.max_depth > 32
        || limits.max_output_bytes == 0
        || limits.max_output_bytes > 64 * 1024 * 1024
        || limits.memory_limit_bytes < 64 * 1024 * 1024
        || limits.memory_limit_bytes > 1024 * 1024 * 1024
        || limits.max_runtime_millis == 0
        || limits.max_runtime_millis > 60_000;
    if invalid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid read request",
        ));
    }
    Ok(())
}

pub(super) fn verify_file(request: &BoltReadRequestDto) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut file = options.open(&request.input_path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() != request.input_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "input size changed",
        ));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if hex::encode(digest.finalize()) != request.input_sha256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "input content changed",
        ));
    }
    Ok(file)
}
