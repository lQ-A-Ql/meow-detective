mod exports;
mod limits;
mod modules;
mod search;
mod utils;

pub use limits::{
    LoadedModuleEntryLayout, TargetedCodeViewIdentity, TargetedKernelDiscovery,
    TargetedKernelIdentity, TargetedKernelLayoutProfile, TargetedKernelPeImage,
    TargetedKernelSearchLimits, TargetedKernelSearchReport,
};
pub use modules::{enumerate_loaded_modules, KernelModule};
pub(crate) use search::read_module_pe_image;
pub use search::{
    discover_kernel_from_entry, discover_kernel_from_processor_start_block, read_codeview_identity,
};
