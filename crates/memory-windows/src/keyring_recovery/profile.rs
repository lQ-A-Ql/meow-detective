use crate::{MemoryWindowsError, Result, TargetedCodeViewIdentity, TargetedKernelLayoutProfile};

const KERNEL_PDB_GUID: &str = "953A8DE8-80B0-818C-32DA-2DEC1D79C2D9";
const FVEVOL_PDB_GUID: &str = "47808A31-873E-98CF-7009-95E410CD0095";

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

/// Exact, reviewed profile for one Windows kernel/fvevol symbol identity.
///
/// The layout is sourced from Microsoft PDBs outside the evidence hot path.
/// Runtime recovery only consumes this local profile after both PE and CodeView
/// identities have matched.
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
    /// Builds the reviewed Windows 11 26100 profile used by the Liu Yang sample.
    pub fn windows_11_26100(kernel: TargetedKernelLayoutProfile) -> Result<Self> {
        require_codeview(kernel.codeview_identity(), KERNEL_PDB_GUID, "ntkrnlmp.pdb")?;
        require_codeview(
            kernel.fvevol_codeview_identity(),
            FVEVOL_PDB_GUID,
            "fvevol.pdb",
        )?;
        Ok(Self {
            kernel,
            objects: ObjectManagerLayout {
                root_directory_object_rva: 0x00F0_DFF0,
                info_mask_to_offset_rva: 0x00F0_E100,
                directory_bucket_count: 37,
                directory_entry_chain_offset: 0,
                directory_entry_object_offset: 8,
                object_header_body_offset: 0x30,
                object_header_info_mask_offset: 0x1A,
                name_info_bit: 0x02,
                name_info_name_offset: 8,
                unicode_length_offset: 0,
                unicode_maximum_length_offset: 2,
                unicode_buffer_offset: 8,
            },
            driver: DriverLayout {
                device_object_offset: 0x08,
                driver_start_offset: 0x18,
                driver_size_offset: 0x20,
                driver_extension_offset: 0x30,
                driver_name_offset: 0x38,
                extension_driver_object_offset: 0,
                extension_client_list_offset: 0x28,
                client_next_offset: 0,
                client_identifier_offset: 8,
                client_body_offset: 0x10,
            },
            keyring: KeyringLayout {
                client_keyring_offset: 0x278,
                capacity: 0x4000,
                header_size: 0x20,
                dataset_minimum_size: 0x30,
                dataset_volume_guid_offset: 0x10,
            },
            devices: DeviceObjectLayout {
                object_type_offset: 0,
                object_size_offset: 2,
                expected_object_type: 3,
                minimum_object_size: 0x50,
                driver_object_offset: 0x08,
                next_device_offset: 0x10,
                device_extension_offset: 0x40,
                maximum_devices: 32,
            },
            volume_context: FveVolumeContextLayout {
                vmk_datum_pointer_offset: 0x3D0,
                vmk_datum_size: 44,
                vmk_datum_entry_type: 0,
                vmk_datum_value_type: 1,
                vmk_datum_version: 0,
                vmk_datum_algorithm: 0x2003,
                vmk_offset: 12,
            },
        })
    }

    /// Offset-blind variant of [`Self::windows_11_26100`]: the two
    /// fvevol-internal offsets (keyring pointer, VMK datum pointer) are zeroed,
    /// forcing discovery onto the signature-anchored bounded scans. This is the
    /// shape multi-version profiles take when fvevol layout offsets are unknown
    /// (public driver PDBs carry no type info), and it doubles as the
    /// regression proving the scans find the same objects as the reviewed
    /// offsets.
    pub fn windows_11_26100_offset_blind(kernel: TargetedKernelLayoutProfile) -> Result<Self> {
        let mut profile = Self::windows_11_26100(kernel)?;
        profile.keyring.client_keyring_offset = 0;
        profile.volume_context.vmk_datum_pointer_offset = 0;
        Ok(profile)
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

fn require_codeview(
    actual: Option<&TargetedCodeViewIdentity>,
    expected_guid: &str,
    expected_name: &str,
) -> Result<()> {
    let matches = actual.is_some_and(|identity| {
        identity.guid() == expected_guid
            && identity.age() == 1
            && identity.pdb_name().eq_ignore_ascii_case(expected_name)
    });
    if matches {
        Ok(())
    } else {
        Err(MemoryWindowsError::UnsupportedBitLockerMemoryProfile)
    }
}
