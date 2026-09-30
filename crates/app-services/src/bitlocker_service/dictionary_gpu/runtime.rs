use super::ffi::{check, gpu_device, Api, Handle};
use sha2::{Digest, Sha256};
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use zeroize::Zeroizing;

const ITERATIONS: u32 = 1_048_576;
const SLICE_ROUNDS: u32 = 4096;
pub(super) const MAX_GPU_CANDIDATES: usize = 2048;
static GPU_BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("No compatible OpenCL GPU runtime is available")]
    Unavailable,
    #[error("Another BitLocker GPU task is running")]
    Busy,
    #[error("The OpenCL GPU failed an operation ({code})")]
    Operation { code: i32 },
    #[error("The OpenCL GPU failed the BitLocker derivation self-test")]
    SelfTest,
}

struct GpuLease;
impl Drop for GpuLease {
    fn drop(&mut self) {
        GPU_BUSY.store(false, Ordering::Release);
    }
}

pub(super) struct GpuRuntime {
    kernel: Handle,
    _program: Handle,
    queue: Handle,
    context: Handle,
    api: Arc<Api>,
    _lease: GpuLease,
}

impl GpuRuntime {
    pub fn new() -> Result<Self, GpuError> {
        GPU_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| GpuError::Busy)?;
        let lease = GpuLease;
        let api = Api::load()?;
        let device = gpu_device(&api)?;
        let mut code = 0;
        // SAFETY: Device ID is runtime-owned; each returned object is immediately
        // wrapped with its matching release function. Source pointer/length remain
        // valid throughout program creation; callbacks and user data are absent.
        unsafe {
            let context = Handle::new(
                (api.context)(
                    std::ptr::null(),
                    1,
                    &device,
                    None,
                    std::ptr::null_mut(),
                    &mut code,
                ),
                code,
                api.release_context,
                &api,
            )?;
            let queue = Handle::new(
                (api.queue)(context.raw, device, 0, &mut code),
                code,
                api.release_queue,
                &api,
            )?;
            let source = include_str!("stretch.cl");
            let pointer = source.as_ptr().cast();
            let len = source.len();
            let program = Handle::new(
                (api.program)(context.raw, 1, &pointer, &len, &mut code),
                code,
                api.release_program,
                &api,
            )?;
            check((api.build)(
                program.raw,
                1,
                &device,
                c"-cl-std=CL1.2".as_ptr(),
                None,
                std::ptr::null_mut(),
            ))?;
            let kernel = Handle::new(
                (api.kernel)(program.raw, c"stretch".as_ptr(), &mut code),
                code,
                api.release_kernel,
                &api,
            )?;
            Ok(Self {
                kernel,
                _program: program,
                queue,
                context,
                api,
                _lease: lease,
            })
        }
    }

    pub fn self_test(&self, initial: &[u32; 8]) -> Result<(), GpuError> {
        let salt = [0x33u8; 16];
        let cancel = AtomicBool::new(false);
        let result = self
            .stretch_n(std::slice::from_ref(initial), &salt, &cancel, 17)?
            .ok_or(GpuError::SelfTest)?;
        let mut reference = Zeroizing::new([0u8; 88]);
        for (i, word) in initial.iter().enumerate() {
            reference[32 + i * 4..36 + i * 4].copy_from_slice(&word.to_be_bytes());
        }
        reference[64..80].copy_from_slice(&salt);
        for count in 0u64..17 {
            reference[80..88].copy_from_slice(&count.to_le_bytes());
            let hash = Zeroizing::new(<[u8; 32]>::from(Sha256::digest(reference.as_slice())));
            reference[..32].copy_from_slice(hash.as_slice());
        }
        if result[0] != reference[..32] {
            return Err(GpuError::SelfTest);
        }
        Ok(())
    }

    pub fn stretch(
        &self,
        hashes: &[[u32; 8]],
        salt: &[u8; 16],
        cancel: &AtomicBool,
    ) -> Result<Option<Zeroizing<Vec<[u8; 32]>>>, GpuError> {
        self.stretch_n(hashes, salt, cancel, ITERATIONS)
    }

    fn stretch_n(
        &self,
        hashes: &[[u32; 8]],
        salt: &[u8; 16],
        cancel: &AtomicBool,
        iterations: u32,
    ) -> Result<Option<Zeroizing<Vec<[u8; 32]>>>, GpuError> {
        if hashes.is_empty() {
            return Ok(Some(Zeroizing::new(Vec::new())));
        }
        if hashes.len() > MAX_GPU_CANDIDATES || iterations > ITERATIONS {
            return Err(GpuError::SelfTest);
        }
        let initial = Zeroizing::new(hashes.iter().flatten().copied().collect::<Vec<_>>());
        let mut output = Zeroizing::new(vec![0u32; initial.len()]);
        let salt_words: [u32; 4] = std::array::from_fn(|i| {
            u32::from_be_bytes([
                salt[i * 4],
                salt[i * 4 + 1],
                salt[i * 4 + 2],
                salt[i * 4 + 3],
            ])
        });
        let input = self.buffer(&initial)?;
        let last = self.buffer(&output)?;
        let salt_buffer = self.buffer(&salt_words)?;
        self.argument(0, &input.handle.raw)?;
        self.argument(1, &last.handle.raw)?;
        self.argument(2, &salt_buffer.handle.raw)?;
        for start in (0..iterations).step_by(SLICE_ROUNDS as usize) {
            if cancel.load(Ordering::Acquire) {
                return Ok(None);
            }
            self.argument(3, &start)?;
            self.argument(4, &SLICE_ROUNDS.min(iterations - start))?;
            self.dispatch(hashes.len())?;
        }
        if cancel.load(Ordering::Acquire) {
            return Ok(None);
        }
        // SAFETY: Output is initialized and has the buffer's exact byte size;
        // blocking read completes before Rust accesses its memory.
        unsafe {
            check((self.api.read)(
                self.queue.raw,
                last.handle.raw,
                1,
                0,
                last.len,
                output.as_mut_ptr().cast(),
                0,
                std::ptr::null(),
                std::ptr::null_mut(),
            ))?;
        }
        Ok(Some(Zeroizing::new(
            output
                .chunks_exact(8)
                .map(|words| {
                    let mut bytes = [0u8; 32];
                    for (i, word) in words.iter().enumerate() {
                        bytes[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
                    }
                    bytes
                })
                .collect(),
        )))
    }

    fn dispatch(&self, global: usize) -> Result<(), GpuError> {
        // SAFETY: Kernel arguments are initialized, global size matches the
        // allocated hash arrays. Each sliced dispatch finishes before buffers
        // are read/reused, bounding watchdog risk and cancellation latency.
        unsafe {
            check((self.api.enqueue)(
                self.queue.raw,
                self.kernel.raw,
                1,
                std::ptr::null(),
                &global,
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null_mut(),
            ))?;
            check((self.api.finish)(self.queue.raw))
        }
    }

    fn argument<T>(&self, index: u32, value: &T) -> Result<(), GpuError> {
        // SAFETY: Private callers pass only initialized ABI values (u32 or Id).
        // OpenCL copies their bytes before this call returns.
        unsafe {
            check((self.api.arg)(
                self.kernel.raw,
                index,
                std::mem::size_of::<T>(),
                (value as *const T).cast(),
            ))
        }
    }

    fn buffer(&self, words: &[u32]) -> Result<SensitiveBuffer<'_>, GpuError> {
        let mut code = 0;
        let len = std::mem::size_of_val(words);
        // SAFETY: COPY_HOST_PTR immediately copies the valid typed slice into
        // a buffer of its exact byte size. The driver does not retain its pointer.
        let raw = unsafe {
            (self.api.buffer)(
                self.context.raw,
                1 | 32,
                len,
                words.as_ptr() as *mut c_void,
                &mut code,
            )
        };
        Ok(SensitiveBuffer {
            handle: Handle::new(raw, code, self.api.release_buffer, &self.api)?,
            runtime: self,
            len,
        })
    }
}

struct SensitiveBuffer<'a> {
    handle: Handle,
    runtime: &'a GpuRuntime,
    len: usize,
}
impl Drop for SensitiveBuffer<'_> {
    fn drop(&mut self) {
        let zero = 0u32;
        // SAFETY: Buffer and queue are owned and alive. OpenCL copies the pattern;
        // finish completes the best-effort zero fill before releasing the buffer.
        unsafe {
            (self.runtime.api.fill)(
                self.runtime.queue.raw,
                self.handle.raw,
                (&zero as *const u32).cast(),
                4,
                0,
                self.len,
                0,
                std::ptr::null(),
                std::ptr::null_mut(),
            );
            (self.runtime.api.finish)(self.runtime.queue.raw);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/bitlocker_dictionary_gpu.rs"]
mod tests;
