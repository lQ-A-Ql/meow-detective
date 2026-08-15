use crate::{targeted_kernel::TargetedKernelLayoutProfile, MemoryWindowsError, Result};

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

/// Recovery profile for one Windows kernel build. The layout layer is
/// version-stable (verified invariant across 741 extracted PDB profiles), so
/// the only per-build data is the object-manager global pair used by the
/// object-directory fast path. That pair is optional: when it is absent,
/// recovery falls back to version-free driver discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitLockerMemoryProfile {
    kernel: TargetedKernelLayoutProfile,
    objects: Option<ObjectManagerLayout>,
    driver: DriverLayout,
    keyring: KeyringLayout,
    devices: DeviceObjectLayout,
    volume_context: FveVolumeContextLayout,
}

impl BitLockerMemoryProfile {
    /// Resolves a profile for one discovered kernel. The CodeView GUID selects
    /// object-manager globals from the embedded symbol registry when the build
    /// is present; unknown builds proceed without them (the object-directory
    /// fast path is skipped in favor of the version-free driver scan). A
    /// missing CodeView identity fails closed with
    /// [`MemoryWindowsError::UnsupportedBitLockerMemoryProfile`].
    pub(crate) fn resolve(kernel: TargetedKernelLayoutProfile) -> Result<Self> {
        let codeview = kernel
            .codeview_identity()
            .ok_or(MemoryWindowsError::UnsupportedBitLockerMemoryProfile)?;
        let objects =
            symbol_table::resolve_ntoskrnl_layouts(codeview.guid()).map(|layouts| layouts.objects);
        Ok(Self {
            kernel,
            objects,
            driver: symbol_table::default_driver_layout(),
            keyring: KeyringLayout {
                client_keyring_offset: 0,
                capacity: 0x4000,
                header_size: 0x20,
                dataset_minimum_size: 0x30,
                dataset_volume_guid_offset: 0x10,
            },
            devices: symbol_table::default_device_layout(),
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

    pub(crate) fn objects(&self) -> Option<ObjectManagerLayout> {
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
