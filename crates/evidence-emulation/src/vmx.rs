use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Component, Path};

use crate::vmdk::VmdkAdapter;
use crate::EmulationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmwareFirmware {
    Bios,
    Efi,
}

impl VmwareFirmware {
    fn value(self) -> &'static str {
        match self {
            Self::Bios => "bios",
            Self::Efi => "efi",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmxConfig {
    disk_path: String,
    disk_adapter: VmdkAdapter,
    recovery_iso_path: Option<String>,
    firmware: VmwareFirmware,
    memory_mib: u32,
    processor_count: u8,
}

impl VmxConfig {
    pub fn new(disk_path: &str, firmware: VmwareFirmware) -> Result<Self, EmulationError> {
        validate_relative_path(disk_path)?;
        Ok(Self {
            disk_path: disk_path.replace('/', "\\"),
            // IDE is present in legacy and modern Windows/PE images without
            // requiring an image-specific VMware SCSI driver.
            disk_adapter: VmdkAdapter::Ide,
            recovery_iso_path: None,
            firmware,
            memory_mib: 4096,
            processor_count: 2,
        })
    }

    pub fn with_recovery_iso(mut self, iso_path: &str) -> Result<Self, EmulationError> {
        validate_recovery_iso_path(iso_path)?;
        self.recovery_iso_path = Some(iso_path.replace('/', "\\"));
        Ok(self)
    }

    pub fn with_disk_adapter(mut self, adapter: VmdkAdapter) -> Self {
        self.disk_adapter = adapter;
        self
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        for (key, value) in self.settings() {
            writeln!(output, "{key} = \"{value}\"").expect("writing to a string cannot fail");
        }
        output
    }

    pub fn validate_rendered(value: &str) -> Result<(), EmulationError> {
        let settings = parse_settings(value)?;
        for (key, expected) in required_security_settings() {
            if settings.get(key).map(String::as_str) != Some(expected) {
                return Err(invalid_vmx(format!("security setting {key} is missing")));
            }
        }
        if settings
            .iter()
            .any(|(key, value)| key.starts_with("ethernet") && value.eq_ignore_ascii_case("TRUE"))
        {
            return Err(invalid_vmx("network adapters must remain disabled"));
        }
        validate_disk_settings(&settings)?;
        validate_recovery_media_settings(&settings)?;
        Ok(())
    }

    fn settings(&self) -> BTreeMap<&'static str, String> {
        let mut values = BTreeMap::new();
        for (key, value) in required_security_settings() {
            values.insert(key, value.to_string());
        }
        values.extend([
            (".encoding", "UTF-8".to_string()),
            ("bios.bootOrder", self.boot_order().to_string()),
            ("config.version", "8".to_string()),
            ("displayName", "Meow Detective Emulation".to_string()),
            ("firmware", self.firmware.value().to_string()),
            ("guestOS", "windows9-64".to_string()),
            ("memsize", self.memory_mib.to_string()),
            ("numvcpus", self.processor_count.to_string()),
            ("virtualHW.version", "16".to_string()),
        ]);
        match self.disk_adapter {
            VmdkAdapter::Ide => values.extend([
                ("ide0:0.deviceType", "disk".to_string()),
                ("ide0:0.fileName", self.disk_path.clone()),
                ("ide0:0.present", "TRUE".to_string()),
            ]),
            VmdkAdapter::LsiLogic => values.extend([
                ("scsi0.present", "TRUE".to_string()),
                ("scsi0.virtualDev", "lsilogic".to_string()),
                ("scsi0:0.deviceType", "disk".to_string()),
                ("scsi0:0.fileName", self.disk_path.clone()),
                ("scsi0:0.present", "TRUE".to_string()),
            ]),
        }
        if let Some(iso_path) = &self.recovery_iso_path {
            values.extend([
                ("ide1:0.deviceType", "cdrom-image".to_string()),
                ("ide1:0.fileName", iso_path.clone()),
                ("ide1:0.present", "TRUE".to_string()),
                ("ide1:0.startConnected", "TRUE".to_string()),
            ]);
        }
        values
    }

    fn boot_order(&self) -> &'static str {
        if self.recovery_iso_path.is_some() {
            "cdrom,hdd"
        } else {
            "hdd"
        }
    }
}

fn required_security_settings() -> [(&'static str, &'static str); 15] {
    [
        ("ethernet0.present", "FALSE"),
        ("floppy0.present", "FALSE"),
        ("isolation.tools.copy.disable", "TRUE"),
        ("isolation.tools.dnd.disable", "TRUE"),
        ("isolation.tools.hgfs.disable", "TRUE"),
        ("isolation.tools.paste.disable", "TRUE"),
        ("isolation.tools.setGUIOptions.enable", "FALSE"),
        ("sharedFolder.maxNum", "0"),
        ("sound.present", "FALSE"),
        ("time.synchronize.continue", "FALSE"),
        ("time.synchronize.restore", "FALSE"),
        ("time.synchronize.resume.disk", "FALSE"),
        ("time.synchronize.shrink", "FALSE"),
        ("tools.syncTime", "FALSE"),
        ("usb.present", "FALSE"),
    ]
}

fn validate_disk_settings(settings: &BTreeMap<String, String>) -> Result<(), EmulationError> {
    let scsi_path = settings.get("scsi0:0.fileName");
    let ide_path = settings.get("ide0:0.fileName");
    match (scsi_path, ide_path) {
        (Some(path), None) => {
            for (key, expected) in [
                ("scsi0.present", "TRUE"),
                ("scsi0.virtualDev", "lsilogic"),
                ("scsi0:0.deviceType", "disk"),
                ("scsi0:0.present", "TRUE"),
            ] {
                if settings.get(key).map(String::as_str) != Some(expected) {
                    return Err(invalid_vmx("SCSI evidence disk settings are incomplete"));
                }
            }
            if settings.keys().any(|key| key.starts_with("ide0")) {
                return Err(invalid_vmx(
                    "SCSI evidence disk must not expose an IDE controller",
                ));
            }
            validate_relative_path(path)
        }
        (None, Some(path)) => {
            for (key, expected) in [("ide0:0.deviceType", "disk"), ("ide0:0.present", "TRUE")] {
                if settings.get(key).map(String::as_str) != Some(expected) {
                    return Err(invalid_vmx("IDE evidence disk settings are incomplete"));
                }
            }
            if settings.keys().any(|key| key.starts_with("scsi0")) {
                return Err(invalid_vmx(
                    "IDE evidence disk must not expose an SCSI controller",
                ));
            }
            validate_relative_path(path)
        }
        (None, None) => Err(invalid_vmx("evidence disk settings are missing")),
        (Some(_), Some(_)) => Err(invalid_vmx("evidence disk must use exactly one controller")),
    }
}

fn parse_settings(value: &str) -> Result<BTreeMap<String, String>, EmulationError> {
    let mut settings = BTreeMap::new();
    for line in value.lines().filter(|line| !line.trim().is_empty()) {
        let (key, raw_value) = line
            .split_once(" = ")
            .ok_or_else(|| invalid_vmx("VMX line is not a key/value setting"))?;
        let parsed = raw_value
            .strip_prefix('"')
            .and_then(|item| item.strip_suffix('"'))
            .ok_or_else(|| invalid_vmx("VMX values must be quoted"))?;
        if settings
            .insert(key.to_string(), parsed.to_string())
            .is_some()
        {
            return Err(invalid_vmx(format!("duplicate VMX setting {key}")));
        }
    }
    Ok(settings)
}

fn validate_relative_path(value: &str) -> Result<(), EmulationError> {
    if value.is_empty()
        || value.contains(['\r', '\n', '"'])
        || value.starts_with(['\\', '/'])
        || value.as_bytes().get(1) == Some(&b':')
    {
        return Err(invalid_vmx("disk path is not a safe relative path"));
    }
    let normalized = value.replace('\\', "/");
    if Path::new(&normalized)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid_vmx("disk path escapes the machine directory"));
    }
    Ok(())
}

fn validate_recovery_media_settings(
    settings: &BTreeMap<String, String>,
) -> Result<(), EmulationError> {
    let Some(path) = settings.get("ide1:0.fileName") else {
        if settings.keys().any(|key| key.starts_with("ide1:0")) {
            return Err(invalid_vmx("recovery media settings are incomplete"));
        }
        if settings.get("bios.bootOrder").map(String::as_str) != Some("hdd") {
            return Err(invalid_vmx("direct boot must select the evidence disk"));
        }
        return Ok(());
    };
    for (key, expected) in [
        ("bios.bootOrder", "cdrom,hdd"),
        ("ide1:0.deviceType", "cdrom-image"),
        ("ide1:0.present", "TRUE"),
        ("ide1:0.startConnected", "TRUE"),
    ] {
        if settings.get(key).map(String::as_str) != Some(expected) {
            return Err(invalid_vmx("recovery media settings are incomplete"));
        }
    }
    validate_recovery_iso_path(path)
}

fn validate_recovery_iso_path(value: &str) -> Result<(), EmulationError> {
    let bytes = value.as_bytes();
    let drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    if !drive_absolute
        || value.contains(['\r', '\n', '"'])
        || !value.to_ascii_lowercase().ends_with(".iso")
    {
        return Err(invalid_vmx("recovery ISO path is invalid"));
    }
    let normalized = value[3..].replace('\\', "/");
    if normalized
        .split('/')
        .any(|component| matches!(component, "" | "." | ".."))
    {
        return Err(invalid_vmx("recovery ISO path is invalid"));
    }
    Ok(())
}

fn invalid_vmx(message: impl Into<String>) -> EmulationError {
    EmulationError::InvalidVmx(message.into())
}
