#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkAdapterInfo {
    pub guid: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub mac_address: Option<String>,
    pub permanent_mac_address: Option<String>,
    pub ip_addresses: Vec<String>,
    pub subnet_masks: Vec<String>,
    pub gateways: Vec<String>,
    pub dhcp_server: Option<String>,
    pub dhcp_enabled: Option<bool>,
    pub dns_servers: Vec<String>,
    pub pnp_instance_id: Option<String>,
    pub service_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ServiceStartType {
    #[default]
    Boot,
    System,
    Automatic,
    AutomaticDelayed,
    Manual,
    Disabled,
    Unknown(u32),
}

impl ServiceStartType {
    pub fn from_raw(start: u32, delayed_auto_start: bool) -> Self {
        match start {
            0 => Self::Boot,
            1 => Self::System,
            2 if delayed_auto_start => Self::AutomaticDelayed,
            2 => Self::Automatic,
            3 => Self::Manual,
            4 => Self::Disabled,
            other => Self::Unknown(other),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Boot => "Boot",
            Self::System => "System",
            Self::Automatic => "Automatic",
            Self::AutomaticDelayed => "Automatic (Delayed Start)",
            Self::Manual => "Manual",
            Self::Disabled => "Disabled",
            Self::Unknown(_) => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ServiceType {
    #[default]
    KernelDriver,
    FileSystemDriver,
    Win32OwnProcess,
    Win32OwnProcessInteractive,
    Win32ShareProcess,
    Win32ShareProcessInteractive,
    Unknown(u32),
}

impl ServiceType {
    pub fn from_raw(raw: u32) -> Self {
        match (raw & 0xff, raw & 0x100 != 0) {
            (0x01, _) => Self::KernelDriver,
            (0x02, _) => Self::FileSystemDriver,
            (0x10, false) => Self::Win32OwnProcess,
            (0x10, true) => Self::Win32OwnProcessInteractive,
            (0x20, false) => Self::Win32ShareProcess,
            (0x20, true) => Self::Win32ShareProcessInteractive,
            _ => Self::Unknown(raw),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::KernelDriver => "Kernel Driver",
            Self::FileSystemDriver => "File System Driver",
            Self::Win32OwnProcess => "Own Process",
            Self::Win32OwnProcessInteractive => "Own Process (Interactive)",
            Self::Win32ShareProcess => "Share Process",
            Self::Win32ShareProcessInteractive => "Share Process (Interactive)",
            Self::Unknown(_) => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemServiceEntry {
    pub service_name: String,
    pub display_name: Option<String>,
    pub image_path: Option<String>,
    pub service_dll: Option<String>,
    pub service_type: ServiceType,
    pub start_type: ServiceStartType,
    pub delayed_auto_start: bool,
    pub error_control: Option<u32>,
    pub group: Option<String>,
    pub object_name: Option<String>,
    pub depend_on_service: Vec<String>,
    pub depend_on_group: Vec<String>,
    pub failure_command: Option<String>,
    pub required_privileges: Vec<String>,
    pub key_path: String,
    pub key_last_write: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SystemServiceInfo {
    pub services: Vec<SystemServiceEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsbDeviceHistoryEntry {
    pub device_name: String,
    pub serial_number: String,
    pub raw_serial_number: String,
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub revision: Option<String>,
    pub first_connect: Option<String>,
    pub last_connect: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MountedDeviceEntry {
    pub device_name: String,
    pub drive_letter: Option<String>,
    pub volume_guid: Option<String>,
    pub disk_signature_hex: Option<String>,
    pub target_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShutdownTimeEntry {
    pub key_path: String,
    pub shutdown_time: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShimCacheEntry {
    pub path: String,
    pub last_modified: Option<String>,
    pub source_key_path: String,
}
