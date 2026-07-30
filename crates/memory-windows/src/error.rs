use std::io;

#[derive(Debug, thiserror::Error)]
pub enum MemoryWindowsError {
    #[error("memory image is empty")]
    EmptyImage,
    #[error("memory image read failed at physical offset {offset:#X}: {source}")]
    PhysicalRead {
        offset: u64,
        #[source]
        source: io::Error,
    },
    #[error("memory image range {offset:#X}..{end:#X} exceeds length {length:#X}")]
    PhysicalOutOfBounds { offset: u64, end: u64, length: u64 },
    #[error("virtual address {address:#X} is not a canonical four-level x64 address")]
    NonCanonicalAddress { address: u64 },
    #[error("page-table entry is not present for virtual address {address:#X}")]
    PageNotPresent { address: u64 },
    #[error("page-table entry for virtual address {address:#X} points outside the memory image")]
    InvalidPageFrame { address: u64 },
    #[error("no valid x64 processor start block was found in the low-memory region")]
    ProcessorStartBlockNotFound,
    #[error("kernel module list is malformed or cyclic")]
    MalformedModuleList,
    #[error("kernel module PE headers are malformed")]
    MalformedPe,
    #[error("targeted BitLocker scan limit is invalid: {reason}")]
    InvalidTargetedScanLimit { reason: &'static str },
    #[error("targeted BitLocker scan exceeded its {resource} limit of {limit}")]
    TargetedScanBudgetExceeded { resource: &'static str, limit: u64 },
    #[error("targeted kernel image was not found within the bounded virtual search")]
    TargetedKernelImageNotFound,
    #[error("targeted kernel discovery uses a different CR3 than the supplied address space")]
    TargetedAddressSpaceMismatch,
    #[error("targeted BitLocker scan did not find fvevol.sys in the trusted module list")]
    TargetedFvevolNotFound,
    #[error("targeted kernel CodeView identity does not match the selected layout profile")]
    TargetedKernelCodeViewMismatch,
    #[error("the selected Windows build does not have a reviewed BitLocker memory profile")]
    UnsupportedBitLockerMemoryProfile,
    #[error("the Windows object directory is malformed or incomplete")]
    MalformedObjectDirectory,
    #[error("the named kernel object '{name}' was not found")]
    NamedKernelObjectNotFound { name: &'static str },
    #[error("the named kernel object '{name}' is ambiguous")]
    AmbiguousNamedKernelObject { name: &'static str },
    #[error("the FVEVol driver object does not match the reviewed profile")]
    MalformedFvevolDriverObject,
    #[error("the FVEVol driver client extension was not found")]
    FvevolClientExtensionNotFound,
    #[error("multiple FVEVol driver client extensions matched the reviewed identity")]
    AmbiguousFvevolClientExtension,
    #[error("the FVEVol BitLocker keyring was not found")]
    BitLockerKeyringNotFound,
    #[error("the FVEVol BitLocker keyring is malformed")]
    MalformedBitLockerKeyring,
    #[error("the BitLocker keyring does not contain the target volume dataset")]
    BitLockerVolumeDatasetNotFound,
    #[error("multiple BitLocker keyring datasets match the target volume")]
    AmbiguousBitLockerVolumeDataset,
    #[error("the target BitLocker keyring dataset does not contain an exact VMK datum")]
    BitLockerVmkDatumNotFound,
    #[error("the target BitLocker keyring dataset contains multiple VMK datums")]
    AmbiguousBitLockerVmkDatum,
    #[error("the FVEVol device-object chain does not match the reviewed profile")]
    MalformedFvevolDeviceChain,
    #[error("no exact VMK datum was present in the reviewed FVEVol volume contexts")]
    BitLockerDeviceVmkNotFound,
}

pub type Result<T> = std::result::Result<T, MemoryWindowsError>;
