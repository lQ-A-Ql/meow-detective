use std::{
    io::{self, Read},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

pub(super) fn capture(
    mut reader: impl Read + Send + 'static,
    limit: u64,
    overflow: Arc<AtomicBool>,
) -> JoinHandle<io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let limit = usize::try_from(limit).map_err(io::Error::other)?;
        let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                return Ok(bytes);
            }
            let length = bytes
                .len()
                .checked_add(count)
                .ok_or_else(|| io::Error::other("output length overflow"))?;
            if length > limit {
                overflow.store(true, Ordering::Release);
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "output limit exceeded",
                ));
            }
            if length > bytes.capacity() {
                let target = bytes.capacity().saturating_mul(2).max(length).min(limit);
                bytes
                    .try_reserve_exact(target - bytes.len())
                    .map_err(io::Error::other)?;
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
    })
}
