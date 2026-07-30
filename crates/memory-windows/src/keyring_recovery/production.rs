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
    let mut scanned = 0;
    let image = read_module_pe_image(address_space, fvevol, limits, &mut scanned)?;
    if Some(image.identity()) != profile.kernel().fvevol_identity() {
        return Err(MemoryWindowsError::TargetedFvevolIdentityMismatch);
    }
    let actual_codeview = read_codeview_identity(address_space, image, limits, &mut scanned)?;
    if actual_codeview.as_ref() != profile.kernel().fvevol_codeview_identity() {
        return Err(MemoryWindowsError::TargetedFvevolCodeViewMismatch);
    }
    Ok(())
}
