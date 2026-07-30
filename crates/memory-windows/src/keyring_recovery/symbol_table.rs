//! Offline ntoskrnl symbol registry: per-build object-manager globals from
//! the harvested PDB collection (see `crates/memory-windows/symbols/README.md`).
//!
//! All struct layouts the recovery path consumes were verified invariant
//! across every extracted profile (1077 builds, Windows 10 10240 through
//! Windows 11 28000), so they live here as reviewed constants. The only
//! per-build data is the object-manager global pair, held in the generated
//! static table. Lookup keys on the CodeView (RSDS) GUID; a missing build
//! fails closed, no layout is ever derived by guessing.

use crate::targeted_kernel::LoadedModuleEntryLayout;

use super::profile::{DeviceObjectLayout, DriverLayout, ObjectManagerLayout};
use super::symbol_registry_generated::EMBEDDED_TABLES;

/// NT object-manager bucket count. Stable across Windows x64 releases and not
/// carried by the extractor's field-level output (array dimensions), so it
/// stays a reviewed constant.
const DIRECTORY_BUCKET_COUNT: u16 = 37;
/// `ObpInfoMaskToOffset` value meaning "object has a name-info header".
/// Algorithmic constant of the object manager, not a PDB fact.
const NAME_INFO_BIT: u8 = 0x02;
/// Device-node object type expected on FVEVol's device chain.
const EXPECTED_DEVICE_TYPE: u16 = 3;
/// Sanity bounds for device objects, policy-level rather than PDB facts.
const MINIMUM_DEVICE_OBJECT_SIZE: u16 = 0x50;
const MAXIMUM_DEVICES: u16 = 32;
/// Undocumented FsRtl filter-client layout shared by FVE drivers. These are
/// reviewed constants; the public PDB names only the
/// `_DRIVER_EXTENSION.ClientDriverExtension` anchor.
const CLIENT_NEXT_OFFSET: u16 = 0;
const CLIENT_IDENTIFIER_OFFSET: u16 = 8;
const CLIENT_BODY_OFFSET: u16 = 0x10;

/// Per-build ntoskrnl layouts resolved from the registry.
pub(crate) struct NtoskrnlLayouts {
    pub build_id: String,
    pub objects: ObjectManagerLayout,
}

/// Resolves layouts for one ntoskrnl CodeView GUID, or `None` when the build
/// is not in the embedded registry.
pub(crate) fn resolve_ntoskrnl_layouts(pdb_guid: &str) -> Option<NtoskrnlLayouts> {
    let table = EMBEDDED_TABLES
        .iter()
        .flat_map(|part| part.iter())
        .find(|table| table.pdb_guid.eq_ignore_ascii_case(pdb_guid))?;
    Some(NtoskrnlLayouts {
        build_id: table.build_id.to_string(),
        objects: ObjectManagerLayout {
            root_directory_object_rva: table.obp_root_rva,
            info_mask_to_offset_rva: table.obp_info_mask_rva,
            directory_bucket_count: DIRECTORY_BUCKET_COUNT,
            // Verified invariant across all 1077 extracted profiles:
            // _OBJECT_DIRECTORY_ENTRY.{ChainLink,Object}, _OBJECT_HEADER.{Body,InfoMask},
            // _OBJECT_HEADER_NAME_INFO.Name, _UNICODE_STRING.{Length,MaximumLength,Buffer}.
            directory_entry_chain_offset: 0,
            directory_entry_object_offset: 8,
            object_header_body_offset: 0x30,
            object_header_info_mask_offset: 0x1A,
            name_info_bit: NAME_INFO_BIT,
            name_info_name_offset: 8,
            unicode_length_offset: 0,
            unicode_maximum_length_offset: 2,
            unicode_buffer_offset: 8,
        },
    })
}

/// Version-stable layouts, verified invariant across all 1077 extracted PDB
/// profiles (Windows 10 10240 through Windows 11 28000). Used when the
/// build's PDB is not in the registry — these need no per-build data.
pub(crate) fn default_driver_layout() -> DriverLayout {
    DriverLayout {
        device_object_offset: 0x08,
        driver_start_offset: 0x18,
        driver_size_offset: 0x20,
        driver_extension_offset: 0x30,
        driver_name_offset: 0x38,
        extension_driver_object_offset: 0,
        extension_client_list_offset: 0x28,
        client_next_offset: CLIENT_NEXT_OFFSET,
        client_identifier_offset: CLIENT_IDENTIFIER_OFFSET,
        client_body_offset: CLIENT_BODY_OFFSET,
    }
}

/// Version-stable device layout (invariant across all extracted profiles).
pub(crate) fn default_device_layout() -> DeviceObjectLayout {
    DeviceObjectLayout {
        object_type_offset: 0,
        object_size_offset: 2,
        expected_object_type: EXPECTED_DEVICE_TYPE,
        minimum_object_size: MINIMUM_DEVICE_OBJECT_SIZE,
        driver_object_offset: 0x08,
        next_device_offset: 0x10,
        device_extension_offset: 0x40,
        maximum_devices: MAXIMUM_DEVICES,
    }
}

/// Version-stable loaded-module entry layout (invariant across all extracted
/// profiles: InLoadOrderLinks@0, DllBase@0x30, SizeOfImage@0x40,
/// BaseDllName@0x58).
pub(crate) fn default_module_layout() -> LoadedModuleEntryLayout {
    LoadedModuleEntryLayout::new(0, 0, 0x30, 0x40, 0x58, 0x60)
        .expect("constant module entry layout is valid")
}
