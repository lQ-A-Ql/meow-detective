use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

// ── MountPoints2 ────────────────────────────────────────────────────────────

pub(super) fn extract_mount_points(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<MountPoint> {
    let mp_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "MountPoints2",
    ];
    let nk = match hive.navigate_to(mp_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("MountPoints2 parse error: {err}"));
            return Vec::new();
        }
    };
    let subkey_names = match hive.read_subkey_names_from_nk(&nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("MountPoints2 subkeys error: {err}"));
            return Vec::new();
        }
    };
    let mut points = Vec::new();
    for name in subkey_names {
        let mut drive_letter = None;
        let mut volume_guid = None;
        if name.len() == 1 && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            drive_letter = Some(format!("{name}:"));
        } else if name.starts_with('{') && name.ends_with('}') {
            volume_guid = Some(name.clone());
        }
        if drive_letter.is_some() || volume_guid.is_some() {
            points.push(MountPoint {
                drive_letter,
                volume_guid,
                last_mounted: None,
            });
        }
    }
    points
}
