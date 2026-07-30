use crate::{MemoryWindowsError, Result, TargetedKernelLayoutProfile};

use super::symbol_table;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObjectManagerLayout {
    pub root_directory_object_rva: u32,
    pub info_mask_to_offset_rva: u32,
    pub directory_bucket_count: u16,
    pub directory_entry_chain_offset: u16,
    pub directory_entry_object_offset: u16,
    pub object_header_body_offset: u16,
    pub object_header_info_mask_offset: u16,
    pub name_info_bit: u8,
    pub name_info_name_offset: u16,
    pub unicode_length_offset: u16,
    pub unicode_maximum_length_offset: u16,
    pub unicode_buffer_offset: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DriverLayout {
    pub device_object_offset: u16,
    pub driver_start_offset: u16,
    pub driver_size_offset: u16,
    pub driver_extension_offset: u16,
    pub driver_name_offset: u16,
    pub extension_driver_object_offset: u16,
    pub extension_client_list_offset: u16,
    pub client_next_offset: u16,
    pub client_identifier_offset: u16,
    pub client_body_offset: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyringLayout {
    pub client_keyring_offset: u16,
    pub capacity: u32,
    pub header_size: u32,
    pub dataset_minimum_size: u32,
    pub dataset_volume_guid_offset: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeviceObjectLayout {
    pub object_type_offset: u16,
    pub object_size_offset: u16,
    pub expected_object_type: u16,
    pub minimum_object_size: u16,
    pub driver_object_offset: u16,
    pub next_device_offset: u16,
    pub device_extension_offset: u16,
    pub maximum_devices: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FveVolumeContextLayout {
    pub vmk_datum_pointer_offset: u16,
    pub vmk_datum_size: u16,
    pub vmk_datum_entry_type: u16,
    pub vmk_datum_value_type: u16,
    pub vmk_datum_version: u16,
    pub vmk_datum_algorithm: u16,
    pub vmk_offset: u16,
}

/// Recovery profile for one Windows kernel build, resolved from the embedded
/// PDB symbol registry. fvevol-internal offsets stay zero: keyring and VMK
/// datum discovery run through signature-anchored bounded scans, so a profile
/// only carries version-stable format constants plus registry-resolved
/// ntoskrnl layouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitLockerMemoryProfile {
    kernel: TargetedKernelLayoutProfile,
    objects: ObjectManagerLayout,
    driver: DriverLayout,
    keyring: KeyringLayout,
    devices: DeviceObjectLayout,
    volume_context: FveVolumeContextLayout,
}

impl BitLockerMemoryProfile {
    /// Resolves a profile for any ntoskrnl build present in the embedded
    /// symbol registry (see `symbol_table`). The CodeView GUID is the only
    /// identity gate; unknown builds fail closed with
    /// [`MemoryWindowsError::UnsupportedBitLockerMemoryProfile`].
    pub fn resolve(kernel: TargetedKernelLayoutProfile) -> Result<Self> {
        let codeview = kernel
            .codeview_identity()
            .ok_or(MemoryWindowsError::UnsupportedBitLockerMemoryProfile)?;
        let layouts = symbol_table::resolve_ntoskrnl_layouts(codeview.guid())
            .ok_or(MemoryWindowsError::UnsupportedBitLockerMemoryProfile)?;
        Ok(Self {
            kernel,
            objects: layouts.objects,
            driver: layouts.driver,
            keyring: KeyringLayout {
                client_keyring_offset: 0,
                capacity: 0x4000,
                header_size: 0x20,
                dataset_minimum_size: 0x30,
                dataset_volume_guid_offset: 0x10,
            },
            devices: layouts.devices,
            volume_context: FveVolumeContextLayout {
                vmk_datum_pointer_offset: 0,
                vmk_datum_size: 44,
                vmk_datum_entry_type: 0,
                vmk_datum_value_type: 1,
                vmk_datum_version: 0,
                vmk_datum_algorithm: 0x2003,
                vmk_offset: 12,
            },
        })
    }

    pub(crate) fn kernel(&self) -> &TargetedKernelLayoutProfile {
        &self.kernel
    }

    pub(crate) fn objects(&self) -> ObjectManagerLayout {
        self.objects
    }

    pub(crate) fn driver(&self) -> DriverLayout {
        self.driver
    }

    pub(crate) fn keyring(&self) -> KeyringLayout {
        self.keyring
    }

    pub(crate) fn devices(&self) -> DeviceObjectLayout {
        self.devices
    }

    pub(crate) fn volume_context(&self) -> FveVolumeContextLayout {
        self.volume_context
    }
}
