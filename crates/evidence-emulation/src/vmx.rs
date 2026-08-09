use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Component, Path};

use crate::vm_options::{
    conditional_security_settings, validate_isolation_exceptions, VmOptions, GUEST_OS_WHITELIST,
};
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
    maintenance_iso_path: Option<String>,
    firmware: VmwareFirmware,
    guest_os: String,
    options: VmOptions,
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
            maintenance_iso_path: None,
            firmware,
            guest_os: "windows9-64".to_string(),
            options: VmOptions::default(),
        })
    }

    /// Set the VMware guest OS identifier (see [`GUEST_OS_WHITELIST`]).
    pub fn with_guest_os(mut self, guest_os: &str) -> Result<Self, EmulationError> {
        if !GUEST_OS_WHITELIST.contains(&guest_os) {
            return Err(invalid_vmx(format!(
                "guest OS '{guest_os}' is not in the supported whitelist"
            )));
        }
        self.guest_os = guest_os.to_string();
        Ok(self)
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

    pub fn with_options(mut self, options: VmOptions) -> Result<Self, EmulationError> {
        options.validate()?;
        self.options = options;
        Ok(self)
    }

    /// Attaches the generated maintenance CD as the second optical drive. The
    /// path is an absolute host path (the image lives in the session
    /// workspace), unlike the relative recovery-media-agnostic disk paths.
    pub fn with_maintenance_iso(mut self, iso_path: &str) -> Result<Self, EmulationError> {
        validate_recovery_iso_path(iso_path)?;
        self.maintenance_iso_path = Some(iso_path.replace('/', "\\"));
        Ok(self)
    }

    pub fn options(&self) -> VmOptions {
        self.options
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        for (key, value) in self.settings() {
            writeln!(output, "{key} = \"{value}\"").expect("writing to a string cannot fail");
        }
        output
    }

    pub fn validate_rendered(
        value: &str,
        options: VmOptions,
        has_maintenance_media: bool,
    ) -> Result<(), EmulationError> {
        let settings = parse_settings(value)?;
        for (key, expected) in base_security_settings()
            .into_iter()
            .chain(conditional_security_settings(options))
        {
            if settings.get(key).map(String::as_str) != Some(expected) {
                return Err(invalid_vmx(format!("security setting {key} is missing")));
            }
        }
        validate_isolation_exceptions(&settings, options)?;
        validate_disk_settings(&settings)?;
        match settings.get("guestOS").map(String::as_str) {
            Some(id) if GUEST_OS_WHITELIST.contains(&id) => {}
            _ => return Err(invalid_vmx("guestOS is not in the supported whitelist")),
        }
        validate_recovery_media_settings(&settings)?;
        validate_maintenance_media_settings(&settings, has_maintenance_media)?;
        Ok(())
    }

    fn settings(&self) -> BTreeMap<&'static str, String> {
        let mut values = BTreeMap::new();
        for (key, value) in base_security_settings()
            .into_iter()
            .chain(conditional_security_settings(self.options))
        {
            values.insert(key, value.to_string());
        }
        values.extend([
            (".encoding", "UTF-8".to_string()),
            ("bios.bootOrder", self.boot_order().to_string()),
            ("config.version", "8".to_string()),
            ("displayName", "Meow Detective Emulation".to_string()),
            ("firmware", self.firmware.value().to_string()),
            ("guestOS", self.guest_os.clone()),
            ("memsize", self.options.memory_mib.to_string()),
            ("numvcpus", self.options.processor_count.to_string()),
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
        if let Some(iso_path) = &self.maintenance_iso_path {
            values.extend([
                ("ide1:1.deviceType", "cdrom-image".to_string()),
                ("ide1:1.fileName", iso_path.clone()),
                ("ide1:1.present", "TRUE".to_string()),
                ("ide1:1.startConnected", "TRUE".to_string()),
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

fn base_security_settings() -> [(&'static str, &'static str); 13] {
    [
        ("floppy0.present", "FALSE"),
        ("isolation.device.connectable.disable", "TRUE"),
        ("isolation.device.edit.disable", "TRUE"),
        ("isolation.tools.getCreds.disable", "TRUE"),
        ("isolation.tools.ghi.autologon.disable", "TRUE"),
        ("isolation.tools.hgfs.disable", "TRUE"),
        ("isolation.tools.hgfsServerSet.disable", "TRUE"),
        ("isolation.tools.memSchedFakeSampleStats.disable", "TRUE"),
        ("isolation.tools.setGUIOptions.enable", "FALSE"),
        ("isolation.tools.unity.push.update.disable", "TRUE"),
        ("sharedFolder.maxNum", "0"),
        ("sound.present", "FALSE"),
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

fn validate_maintenance_media_settings(
    settings: &BTreeMap<String, String>,
    has_maintenance_media: bool,
) -> Result<(), EmulationError> {
    let path = settings.get("ide1:1.fileName");
    match (path, has_maintenance_media) {
        (Some(path), true) => {
            for (key, expected) in [
                ("ide1:1.deviceType", "cdrom-image"),
                ("ide1:1.present", "TRUE"),
                ("ide1:1.startConnected", "TRUE"),
            ] {
                if settings.get(key).map(String::as_str) != Some(expected) {
                    return Err(invalid_vmx("maintenance media settings are incomplete"));
                }
            }
            validate_recovery_iso_path(path)
        }
        (None, false) => {
            if settings.keys().any(|key| key.starts_with("ide1:1")) {
                return Err(invalid_vmx("maintenance media settings are incomplete"));
            }
            Ok(())
        }
        (Some(_), false) => Err(invalid_vmx("unexpected maintenance media attachment")),
        (None, true) => Err(invalid_vmx("maintenance media attachment is missing")),
    }
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

pub(crate) fn invalid_vmx(message: impl Into<String>) -> EmulationError {
    EmulationError::InvalidVmx(message.into())
}
