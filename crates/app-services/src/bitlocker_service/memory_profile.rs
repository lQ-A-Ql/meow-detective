use std::path::Path;

use memory_windows::{resolve_profile_for_image, BitLockerMemoryProfile, MemoryWindowsError};

/// Resolves the recovery profile from the memory image itself: kernel
/// discovery + CodeView identity + embedded PDB symbol registry. Any
/// ntoskrnl build present in the registry is supported; unknown builds fail
/// closed with `UnsupportedBitLockerMemoryProfile`.
pub(super) fn resolve_memory_profile(
    memory_image_path: &Path,
) -> Result<BitLockerMemoryProfile, MemoryWindowsError> {
    resolve_profile_for_image(memory_image_path)
}
