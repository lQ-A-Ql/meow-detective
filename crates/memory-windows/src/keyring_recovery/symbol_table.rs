//! Offline ntoskrnl symbol registry: reviewed per-build layouts resolved from
//! whitelist JSON tables extracted from official Microsoft PDBs (see
//! `crates/memory-windows/symbols/README.md`).
//!
//! Lookup keys on the CodeView (RSDS) GUID. A missing build fails closed; no
//! layout is ever derived by guessing.

use serde_json::Value;

use crate::targeted_kernel::LoadedModuleEntryLayout;

use super::profile::{DeviceObjectLayout, DriverLayout, ObjectManagerLayout};

/// NT object-manager bucket count. Stable across Windows x64 releases and not
/// carried by the extractor's field-level JSON (array dimensions), so it
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

/// Per-build ntoskrnl layouts resolved from an embedded symbol table. Only
/// the object-manager globals and the build label are consumed today; the
/// remaining layout fields are resolved on demand through the same table.
pub(crate) struct NtoskrnlLayouts {
    pub build_id: String,
    pub objects: ObjectManagerLayout,
    pub module_layout: LoadedModuleEntryLayout,
}

/// Version-stable layouts, verified invariant across 741 extracted PDB
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

struct EmbeddedTable {
    pdb_guid: &'static str,
    json: &'static str,
}

const EMBEDDED_TABLES: &[EmbeddedTable] = &[EmbeddedTable {
    pdb_guid: "953A8DE8-80B0-818C-32DA-2DEC1D79C2D9",
    json: include_str!(
        "../../symbols/windows/ntkrnlmp/953A8DE8-80B0-818C-32DA-2DEC1D79C2D9-6.json"
    ),
}];

/// Resolves layouts for one ntoskrnl CodeView GUID, or `None` when the build
/// is not in the embedded registry.
pub(crate) fn resolve_ntoskrnl_layouts(pdb_guid: &str) -> Option<NtoskrnlLayouts> {
    let table = EMBEDDED_TABLES
        .iter()
        .find(|table| table.pdb_guid.eq_ignore_ascii_case(pdb_guid))?;
    let root: Value = serde_json::from_str(table.json).ok()?;
    layouts_from_table(root, table.pdb_guid)
}

fn layouts_from_table(root: Value, pdb_guid: &str) -> Option<NtoskrnlLayouts> {
    let objects = ObjectManagerLayout {
        root_directory_object_rva: global_rva(&root, "ObpRootDirectoryObject")?,
        info_mask_to_offset_rva: global_rva(&root, "ObpInfoMaskToOffset")?,
        directory_bucket_count: DIRECTORY_BUCKET_COUNT,
        directory_entry_chain_offset: field_offset(&root, "_OBJECT_DIRECTORY_ENTRY", "ChainLink")?,
        directory_entry_object_offset: field_offset(&root, "_OBJECT_DIRECTORY_ENTRY", "Object")?,
        object_header_body_offset: field_offset(&root, "_OBJECT_HEADER", "Body")?,
        object_header_info_mask_offset: field_offset(&root, "_OBJECT_HEADER", "InfoMask")?,
        name_info_bit: NAME_INFO_BIT,
        name_info_name_offset: field_offset(&root, "_OBJECT_HEADER_NAME_INFO", "Name")?,
        unicode_length_offset: field_offset(&root, "_UNICODE_STRING", "Length")?,
        unicode_maximum_length_offset: field_offset(&root, "_UNICODE_STRING", "MaximumLength")?,
        unicode_buffer_offset: field_offset(&root, "_UNICODE_STRING", "Buffer")?,
    };
    let base_dll_name = field_offset(&root, "_KLDR_DATA_TABLE_ENTRY", "BaseDllName")?;
    let module_layout = LoadedModuleEntryLayout::new(
        0,
        field_offset(&root, "_KLDR_DATA_TABLE_ENTRY", "InLoadOrderLinks")?,
        field_offset(&root, "_KLDR_DATA_TABLE_ENTRY", "DllBase")?,
        field_offset(&root, "_KLDR_DATA_TABLE_ENTRY", "SizeOfImage")?,
        base_dll_name,
        base_dll_name.checked_add(8)?,
    )
    .ok()?;
    let build_id = root
        .get("pdbAge")
        .and_then(Value::as_u64)
        .map(|age| format!("pdb-age-{age}"))
        .unwrap_or_else(|| pdb_guid.to_string());
    Some(NtoskrnlLayouts {
        build_id,
        objects,
        module_layout,
    })
}

fn global_rva(root: &Value, name: &str) -> Option<u32> {
    let rva = root.get("globals")?.get(name)?.get("rva")?.as_u64()?;
    u32::try_from(rva).ok()
}

fn field_offset(root: &Value, type_name: &str, field_name: &str) -> Option<u16> {
    let fields = root
        .get("types")?
        .get(type_name)?
        .get("fields")?
        .as_array()?;
    let offset = fields
        .iter()
        .find(|field| field.get("name").and_then(Value::as_str) == Some(field_name))?
        .get("offset")?
        .as_u64()?;
    u16::try_from(offset).ok()
}
