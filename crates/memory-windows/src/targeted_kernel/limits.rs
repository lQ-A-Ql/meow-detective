use crate::Result;

pub(crate) const MAX_KERNEL_SEARCH_PAGES: usize = 131_072;
pub(crate) const MAX_KERNEL_SEARCH_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_IMAGE_SIZE: u32 = 512 * 1024 * 1024;
pub(crate) const MAX_PE_HEADER_BYTES: usize = 0x1000;
pub(crate) const MAX_EXPORT_ENTRIES: usize = 65_536;
pub(crate) const MAX_MODULES: usize = 1_024;
pub(crate) const MAX_MODULE_NAME_BYTES: usize = 512;
pub(crate) const PE_EXPORT_DIRECTORY_LEN: usize = 40;
pub(crate) const MAX_CODEVIEW_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetedKernelSearchLimits {
    pub(crate) maximum_pages: usize,
    pub(crate) maximum_scanned_bytes: u64,
    pub(crate) maximum_export_entries: usize,
    pub(crate) maximum_modules: usize,
    pub(crate) maximum_module_name_bytes: usize,
}

impl TargetedKernelSearchLimits {
    pub fn new(
        maximum_pages: usize,
        maximum_scanned_bytes: u64,
        maximum_export_entries: usize,
        maximum_modules: usize,
        maximum_module_name_bytes: usize,
    ) -> Result<Self> {
        super::utils::validate_limit(
            (1..=MAX_KERNEL_SEARCH_PAGES).contains(&maximum_pages),
            "kernel page count is outside the hard ceiling",
        )?;
        super::utils::validate_limit(
            (1..=MAX_KERNEL_SEARCH_BYTES).contains(&maximum_scanned_bytes),
            "kernel byte budget is outside the hard ceiling",
        )?;
        super::utils::validate_limit(
            (1..=MAX_EXPORT_ENTRIES).contains(&maximum_export_entries),
            "export entry count is outside the hard ceiling",
        )?;
        super::utils::validate_limit(
            (1..=MAX_MODULES).contains(&maximum_modules),
            "module count is outside the hard ceiling",
        )?;
        super::utils::validate_limit(
            (2..=MAX_MODULE_NAME_BYTES).contains(&maximum_module_name_bytes),
            "module name length is outside the hard ceiling",
        )?;
        Ok(Self {
            maximum_pages,
            maximum_scanned_bytes,
            maximum_export_entries,
            maximum_modules,
            maximum_module_name_bytes,
        })
    }
}

impl Default for TargetedKernelSearchLimits {
    fn default() -> Self {
        Self {
            maximum_pages: 16_384,
            maximum_scanned_bytes: 8 * 1024 * 1024,
            maximum_export_entries: 16_384,
            maximum_modules: 256,
            maximum_module_name_bytes: 512,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetedKernelSearchReport {
    pub pages_scanned: usize,
    pub bytes_scanned: u64,
    pub unreadable_pages: usize,
    pub rejected_pe_candidates: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetedKernelPeImage {
    pub base: u64,
    pub size_of_image: u32,
    pub(crate) export_rva: u32,
    pub(crate) export_size: u32,
    pub(crate) debug_rva: u32,
    pub(crate) debug_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetedKernelDiscovery {
    pub image: TargetedKernelPeImage,
    pub ps_loaded_module_list: u64,
    pub report: TargetedKernelSearchReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetedCodeViewIdentity {
    guid: String,
    age: u32,
    pdb_name: String,
}

impl TargetedCodeViewIdentity {
    pub fn new(guid: impl Into<String>, age: u32, pdb_name: impl Into<String>) -> Result<Self> {
        let guid = guid.into().to_ascii_uppercase();
        let pdb_name = pdb_name.into();
        super::utils::validate_limit(
            is_codeview_guid(&guid),
            "CodeView GUID is not a canonical identifier",
        )?;
        super::utils::validate_limit(
            !pdb_name.trim().is_empty() && !pdb_name.chars().any(char::is_control),
            "CodeView PDB name is empty or contains control characters",
        )?;
        Ok(Self {
            guid,
            age,
            pdb_name,
        })
    }

    #[must_use]
    pub fn guid(&self) -> &str {
        &self.guid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadedModuleEntryLayout {
    pub link_to_entry: i32,
    pub flink_offset: u16,
    pub dll_base_offset: u16,
    pub size_of_image_offset: u16,
    pub name_length_offset: u16,
    pub name_buffer_offset: u16,
}

/// Explicit build/profile binding for the loaded-module list walk.
///
/// The CodeView identity is the only identity gate; the loaded-module entry
/// layout itself is supplied by the caller (the registry default has been
/// verified invariant across all harvested profiles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetedKernelLayoutProfile {
    profile_id: String,
    build_id: String,
    codeview_identity: Option<TargetedCodeViewIdentity>,
    module_layout: LoadedModuleEntryLayout,
}

impl TargetedKernelLayoutProfile {
    pub fn new(
        profile_id: impl Into<String>,
        build_id: impl Into<String>,
        module_layout: LoadedModuleEntryLayout,
    ) -> Result<Self> {
        let profile_id = profile_id.into();
        let build_id = build_id.into();
        super::utils::validate_limit(
            !profile_id.trim().is_empty() && !build_id.trim().is_empty(),
            "kernel layout profile and build identifiers must be non-empty",
        )?;
        super::utils::validate_limit(
            !profile_id.chars().any(char::is_control) && !build_id.chars().any(char::is_control),
            "kernel layout identifiers must not contain control characters",
        )?;
        Ok(Self {
            profile_id,
            build_id,
            codeview_identity: None,
            module_layout,
        })
    }

    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    #[must_use]
    pub fn build_id(&self) -> &str {
        &self.build_id
    }

    #[must_use]
    pub fn with_codeview_identity(mut self, identity: TargetedCodeViewIdentity) -> Self {
        self.codeview_identity = Some(identity);
        self
    }

    #[must_use]
    pub(crate) fn codeview_identity(&self) -> Option<&TargetedCodeViewIdentity> {
        self.codeview_identity.as_ref()
    }

    #[must_use]
    pub(crate) fn module_layout(&self) -> LoadedModuleEntryLayout {
        self.module_layout
    }
}

fn is_codeview_guid(value: &str) -> bool {
    value.len() == 36
        && [8, 13, 18, 23]
            .into_iter()
            .all(|index| value.as_bytes()[index] == b'-')
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
}

impl LoadedModuleEntryLayout {
    pub fn new(
        link_to_entry: i32,
        flink_offset: u16,
        dll_base_offset: u16,
        size_of_image_offset: u16,
        name_length_offset: u16,
        name_buffer_offset: u16,
    ) -> Result<Self> {
        super::utils::validate_limit(
            (-(crate::physical::PAGE_SIZE as i32)..=crate::physical::PAGE_SIZE as i32)
                .contains(&link_to_entry),
            "module link-to-entry offset exceeds the bounded profile",
        )?;
        for (offset, width) in [
            (flink_offset, 8),
            (dll_base_offset, 8),
            (size_of_image_offset, 4),
            (name_length_offset, 2),
            (name_buffer_offset, 8),
        ] {
            super::utils::validate_limit(
                usize::from(offset) + width <= crate::physical::PAGE_SIZE,
                "module layout field exceeds one page",
            )?;
        }
        Ok(Self {
            link_to_entry,
            flink_offset,
            dll_base_offset,
            size_of_image_offset,
            name_length_offset,
            name_buffer_offset,
        })
    }
}
