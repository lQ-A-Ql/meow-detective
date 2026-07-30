use memory_windows::{
    BitLockerMemoryProfile, LoadedModuleEntryLayout, MemoryWindowsError, TargetedCodeViewIdentity,
    TargetedKernelIdentity, TargetedKernelLayoutProfile,
};

pub(super) fn reviewed_windows_11_26100_profile(
) -> Result<BitLockerMemoryProfile, MemoryWindowsError> {
    let module_layout = LoadedModuleEntryLayout::new(0, 0, 0x30, 0x40, 0x58, 0x60)?;
    let profile = TargetedKernelLayoutProfile::new(
        "windows-11-26100-ntkrnlmp-953a8de8",
        "26100",
        TargetedKernelIdentity::new(0xD98D_B6A6, 0x0144_F000),
        module_layout,
    )?
    .with_codeview_identity(TargetedCodeViewIdentity::new(
        "953A8DE8-80B0-818C-32DA-2DEC1D79C2D9",
        1,
        "ntkrnlmp.pdb",
    )?)
    .with_fvevol_identity(TargetedKernelIdentity::new(0x5960_C289, 0x000E_1000))
    .with_fvevol_codeview_identity(TargetedCodeViewIdentity::new(
        "47808A31-873E-98CF-7009-95E410CD0095",
        1,
        "fvevol.pdb",
    )?);
    BitLockerMemoryProfile::windows_11_26100(profile)
}
