//! Optional OpenCL KDF acceleration; all volume authentication stays in Rust.
mod ffi;
mod runtime;
mod worker;

pub use runtime::GpuError;
pub(super) use worker::GpuWorkers;
