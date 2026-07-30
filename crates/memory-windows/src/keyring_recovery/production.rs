use std::path::Path;

use crate::{
    bootstrap::find_processor_start_blocks,
    targeted_kernel::{
        discover_kernel_from_processor_start_block, enumerate_loaded_modules,
        read_codeview_identity, read_module_pe_image, KernelModule, TargetedKernelDiscovery,
        TargetedKernelSearchLimits,
    },
    MemoryWindowsError, RawMemoryImage, Result, X64AddressSpace,
};

use super::{
    driver::find_keyring,
    keyring::read_matching_vmk,
    object_directory::{find_named_object, root_directory},
    profile::BitLockerMemoryProfile,
    report::BitLockerMemoryRecovery,
    symbol_table,
    volume_context::read_device_context_vmks,
};

const MAXIMUM_PHYSICAL_READ_OPERATIONS: u64 = 16_384;
const MAXIMUM_PHYSICAL_READ_BYTES: u64 = 32 * 1024 * 1024;

struct ProfiledMemorySession {
    address_space: X64AddressSpace,
    kernel: TargetedKernelDiscovery,
    fvevol: KernelModule,
}

/// Recovers structurally sourced VMKs through exact kernel and FVEVol objects.
///
/// No writable-section scan, pointer graph, pool-tag scan, AES schedule scan, or
/// key pairing is performed. Unknown PE/PDB identities fail closed.
pub fn recover_vmks_structurally(
    path: &Path,
    profile: &BitLockerMemoryProfile,
    volume_guid: [u8; 16],
    limits: TargetedKernelSearchLimits,
) -> Result<BitLockerMemoryRecovery> {
    let mut session = open_profiled_session(path, profile, limits)?;
    let root = root_directory(
        &mut session.address_space,
        session.kernel.image.base,
        profile.objects(),
    )?;
    let driver_directory = find_named_object(
        &mut session.address_space,
        session.kernel.image.base,
        root,
        "Driver",
        profile.objects(),
    )?;
    let fvevol_driver = find_named_object(
        &mut session.address_space,
        session.kernel.image.base,
        driver_directory,
        "FVEVol",
        profile.objects(),
    )?;
    let keyring_result = find_keyring(
        &mut session.address_space,
        fvevol_driver,
        session.fvevol.base,
        session.fvevol.size,
        profile.driver(),
        profile.keyring(),
    )
    .and_then(|keyring| {
        read_matching_vmk(
            &mut session.address_space,
            keyring,
            volume_guid,
            profile.keyring(),
        )
    });
    match keyring_result {
        Ok(parsed) => Ok(BitLockerMemoryRecovery::new(
            vec![parsed.vmk],
            profile.kernel().profile_id().to_string(),
            profile.kernel().build_id().to_string(),
            parsed.datasets_examined,
            0,
            0,
            session.address_space.physical_read_stats(),
        )),
        Err(
            MemoryWindowsError::BitLockerKeyringNotFound
            | MemoryWindowsError::FvevolClientExtensionNotFound
            | MemoryWindowsError::BitLockerVolumeDatasetNotFound,
        ) => {
            let recovered = read_device_context_vmks(
                &mut session.address_space,
                fvevol_driver,
                profile.driver(),
                profile.devices(),
                profile.volume_context(),
            )?;
            Ok(BitLockerMemoryRecovery::new(
                recovered.vmks,
                profile.kernel().profile_id().to_string(),
                profile.kernel().build_id().to_string(),
                0,
                recovered.devices_examined,
                recovered.datum_pointers_examined,
                session.address_space.physical_read_stats(),
            ))
        }
        Err(error) => Err(error),
    }
}

/// Resolves the recovery profile directly from the memory image: discovers
/// the kernel, reads its CodeView identity, and resolves layouts from the
/// embedded PDB symbol registry. The ntoskrnl CodeView GUID is the only
/// identity gate; unknown builds fail closed with
/// [`MemoryWindowsError::UnsupportedBitLockerMemoryProfile`]. The fvevol
/// identity is not gated — it is anchored structurally by the driver object
/// and the signature scans.
pub fn resolve_profile_for_image(path: &Path) -> Result<BitLockerMemoryProfile> {
    let mut image = RawMemoryImage::open(path)?;
    image.set_read_budget(
        MAXIMUM_PHYSICAL_READ_OPERATIONS,
        MAXIMUM_PHYSICAL_READ_BYTES,
    )?;
    let processor = find_processor_start_blocks(&mut image)?
        .into_iter()
        .next()
        .ok_or(MemoryWindowsError::ProcessorStartBlockNotFound)?;
    let mut address_space = X64AddressSpace::new(image, processor.directory_table_base)?;
    let limits = TargetedKernelSearchLimits::default();
    let kernel = discover_kernel_from_processor_start_block(&mut address_space, processor, limits)?;
    let mut scanned = 0;
    let codeview = read_codeview_identity(&mut address_space, kernel.image, limits, &mut scanned)?
        .ok_or(MemoryWindowsError::UnsupportedBitLockerMemoryProfile)?;
    let layouts = symbol_table::resolve_ntoskrnl_layouts(codeview.guid())
        .ok_or(MemoryWindowsError::UnsupportedBitLockerMemoryProfile)?;
    let kernel_profile = crate::targeted_kernel::TargetedKernelLayoutProfile::new(
        format!("ntoskrnl-{}", codeview.guid()),
        layouts.build_id.clone(),
        kernel.image.identity(),
        layouts.module_layout,
    )?
    .with_codeview_identity(codeview);
    BitLockerMemoryProfile::resolve(kernel_profile)
}

fn open_profiled_session(
    path: &Path,
    profile: &BitLockerMemoryProfile,
    limits: TargetedKernelSearchLimits,
) -> Result<ProfiledMemorySession> {
    let mut image = RawMemoryImage::open(path)?;
    image.set_read_budget(
        MAXIMUM_PHYSICAL_READ_OPERATIONS,
        MAXIMUM_PHYSICAL_READ_BYTES,
    )?;
    let processor = find_processor_start_blocks(&mut image)?
        .into_iter()
        .next()
        .ok_or(MemoryWindowsError::ProcessorStartBlockNotFound)?;
    let mut address_space = X64AddressSpace::new(image, processor.directory_table_base)?;
    let kernel = discover_kernel_from_processor_start_block(&mut address_space, processor, limits)?;
    validate_kernel_identity(&mut address_space, &kernel, profile, limits)?;
    let fvevol = find_fvevol_module(&mut address_space, &kernel, profile, limits)?;
    validate_fvevol_identity(&mut address_space, &fvevol, profile, limits)?;
    Ok(ProfiledMemorySession {
        address_space,
        kernel,
        fvevol,
    })
}

fn validate_kernel_identity(
    address_space: &mut X64AddressSpace,
    kernel: &TargetedKernelDiscovery,
    profile: &BitLockerMemoryProfile,
    limits: TargetedKernelSearchLimits,
) -> Result<()> {
    let expected = profile.kernel().kernel_identity();
    let actual = kernel.image.identity();
    if actual != expected {
        return Err(MemoryWindowsError::TargetedKernelIdentityMismatch {
            expected_timestamp: expected.time_date_stamp(),
            expected_size: expected.size_of_image(),
            actual_timestamp: actual.time_date_stamp(),
            actual_size: actual.size_of_image(),
        });
    }
    let mut scanned = 0;
    let actual_codeview =
        read_codeview_identity(address_space, kernel.image, limits, &mut scanned)?;
    if actual_codeview.as_ref() != profile.kernel().codeview_identity() {
        return Err(MemoryWindowsError::TargetedKernelCodeViewMismatch);
    }
    Ok(())
}

fn find_fvevol_module(
    address_space: &mut X64AddressSpace,
    kernel: &TargetedKernelDiscovery,
    profile: &BitLockerMemoryProfile,
    limits: TargetedKernelSearchLimits,
) -> Result<KernelModule> {
    enumerate_loaded_modules(
        address_space,
        kernel.ps_loaded_module_list,
        profile.kernel().module_layout(),
        limits,
    )?
    .into_iter()
    .find(|module| module.name.eq_ignore_ascii_case("fvevol.sys"))
    .ok_or(MemoryWindowsError::TargetedFvevolNotFound)
}

fn validate_fvevol_identity(
    address_space: &mut X64AddressSpace,
    fvevol: &KernelModule,
    profile: &BitLockerMemoryProfile,
    limits: TargetedKernelSearchLimits,
) -> Result<()> {
    // fvevol identity checks are optional: profiles resolved from the symbol
    // registry anchor the driver structurally (name, image base/size) and do
    // not carry expected fvevol PE/PDB identities. The reviewed 26100 profile
    // still enforces both.
    if profile.kernel().fvevol_identity().is_none()
        && profile.kernel().fvevol_codeview_identity().is_none()
    {
        return Ok(());
    }
    let mut scanned = 0;
    let image = read_module_pe_image(address_space, fvevol, limits, &mut scanned)?;
    if let Some(expected) = profile.kernel().fvevol_identity() {
        if image.identity() != expected {
            return Err(MemoryWindowsError::TargetedFvevolIdentityMismatch);
        }
    }
    if let Some(expected_codeview) = profile.kernel().fvevol_codeview_identity() {
        let actual_codeview = read_codeview_identity(address_space, image, limits, &mut scanned)?;
        if actual_codeview.as_ref() != Some(expected_codeview) {
            return Err(MemoryWindowsError::TargetedFvevolCodeViewMismatch);
        }
    }
    Ok(())
}
