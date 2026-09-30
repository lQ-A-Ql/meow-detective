//! Minimal OpenCL 1.2 ABI, dynamically loaded from the system runtime.
use libloading::Library;
use std::ffi::{c_char, c_void};
use std::sync::Arc;

use super::runtime::GpuError;
pub(super) type Id = *mut c_void;
pub(super) type Release = unsafe extern "system" fn(Id) -> i32;

macro_rules! api {
    ($($field:ident: $name:literal ($($arg:ty),*) -> $result:ty;)+) => {
        pub(super) struct Api {
            _library: Library,
            $(pub $field: unsafe extern "system" fn($($arg),*) -> $result,)+
        }
        impl Api {
            pub fn load() -> Result<Arc<Self>, GpuError> {
                let library = load_library()?;
                // SAFETY: Signatures below match the OpenCL 1.2 C ABI. The
                // library remains alive for the entire Api/handle lifetime.
                unsafe {
                    $(let $field = *library.get(concat!($name, "\0").as_bytes())
                        .map_err(|_| GpuError::Unavailable)?;)+
                    Ok(Arc::new(Self { _library: library, $($field,)+ }))
                }
            }
        }
    }
}

api! {
    platforms: "clGetPlatformIDs"(u32, *mut Id, *mut u32) -> i32;
    devices: "clGetDeviceIDs"(Id, u64, u32, *mut Id, *mut u32) -> i32;
    device_info: "clGetDeviceInfo"(Id, u32, usize, *mut c_void, *mut usize) -> i32;
    context: "clCreateContext"(*const isize, u32, *const Id, Option<unsafe extern "system" fn(*const c_char, *const c_void, usize, *mut c_void)>, *mut c_void, *mut i32) -> Id;
    queue: "clCreateCommandQueue"(Id, Id, u64, *mut i32) -> Id;
    program: "clCreateProgramWithSource"(Id, u32, *const *const c_char, *const usize, *mut i32) -> Id;
    build: "clBuildProgram"(Id, u32, *const Id, *const c_char, Option<unsafe extern "system" fn(Id, *mut c_void)>, *mut c_void) -> i32;
    kernel: "clCreateKernel"(Id, *const c_char, *mut i32) -> Id;
    buffer: "clCreateBuffer"(Id, u64, usize, *mut c_void, *mut i32) -> Id;
    arg: "clSetKernelArg"(Id, u32, usize, *const c_void) -> i32;
    enqueue: "clEnqueueNDRangeKernel"(Id, Id, u32, *const usize, *const usize, *const usize, u32, *const Id, *mut Id) -> i32;
    read: "clEnqueueReadBuffer"(Id, Id, u32, usize, usize, *mut c_void, u32, *const Id, *mut Id) -> i32;
    fill: "clEnqueueFillBuffer"(Id, Id, *const c_void, usize, usize, usize, u32, *const Id, *mut Id) -> i32;
    finish: "clFinish"(Id) -> i32;
    release_context: "clReleaseContext"(Id) -> i32;
    release_queue: "clReleaseCommandQueue"(Id) -> i32;
    release_program: "clReleaseProgram"(Id) -> i32;
    release_kernel: "clReleaseKernel"(Id) -> i32;
    release_buffer: "clReleaseMemObject"(Id) -> i32;
}

#[cfg(windows)]
fn load_library() -> Result<Library, GpuError> {
    // SAFETY: Restrict loading to System32, avoiding DLL lookup in evidence or
    // current-working directories. No code from an investigator path is loaded.
    unsafe { libloading::os::windows::Library::load_with_flags("OpenCL.dll", 0x0000_0800) }
        .map(Library::from)
        .map_err(|_| GpuError::Unavailable)
}

#[cfg(not(windows))]
fn load_library() -> Result<Library, GpuError> {
    // SAFETY: The platform dynamic linker resolves its installed OpenCL runtime.
    unsafe { Library::new("libOpenCL.so.1") }.map_err(|_| GpuError::Unavailable)
}

pub(super) struct Handle {
    pub raw: Id,
    release: Release,
    _api: Arc<Api>,
}

impl Handle {
    pub fn new(raw: Id, code: i32, release: Release, api: &Arc<Api>) -> Result<Self, GpuError> {
        if raw.is_null() {
            return Err(GpuError::Operation { code });
        }
        let handle = Self {
            raw,
            release,
            _api: Arc::clone(api),
        };
        check(code)?;
        Ok(handle)
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: Each non-null OpenCL object has exactly one owning Handle,
        // the corresponding release function and an alive runtime library.
        unsafe {
            (self.release)(self.raw);
        }
    }
}

pub(super) fn check(code: i32) -> Result<(), GpuError> {
    if code == 0 {
        Ok(())
    } else {
        Err(GpuError::Operation { code })
    }
}

pub(super) fn gpu_device(api: &Api) -> Result<Id, GpuError> {
    let mut count = 0;
    // SAFETY: Counts and bounded output arrays have the exact ABI sizes. Device
    // and platform IDs are borrowed runtime-owned objects. GPU type excludes CPUs.
    unsafe {
        let status = (api.platforms)(0, std::ptr::null_mut(), &mut count);
        if status == -1001 {
            return Err(GpuError::Unavailable);
        }
        check(status)?;
        if count == 0 || count > 64 {
            return Err(GpuError::Unavailable);
        }
        let mut platforms = vec![std::ptr::null_mut(); count as usize];
        check((api.platforms)(
            count,
            platforms.as_mut_ptr(),
            std::ptr::null_mut(),
        ))?;
        for platform in platforms {
            let mut device = std::ptr::null_mut();
            if (api.devices)(platform, 4, 1, &mut device, std::ptr::null_mut()) == 0
                && !device.is_null()
            {
                let mut little_endian = 0u32;
                check((api.device_info)(
                    device,
                    0x1026,
                    4,
                    (&mut little_endian as *mut u32).cast(),
                    std::ptr::null_mut(),
                ))?;
                if (little_endian != 0) == cfg!(target_endian = "little") {
                    return Ok(device);
                }
            }
        }
    }
    Err(GpuError::Unavailable)
}
