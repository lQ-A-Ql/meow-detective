use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

/// Extract the shutdown time from `SYSTEM\<ControlSet>\Control\Windows\ShutdownTime`.
///
/// The value may be stored as `REG_BINARY` (8-byte FILETIME) or `REG_QWORD`; both are
/// accepted and converted to an RFC 3339 timestamp.
pub fn extract_shutdown_time_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<ShutdownTimeEntry>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut warnings = Vec::new();
    let control_sets = hive.control_set_candidates(&mut warnings);
    let mut entries = Vec::new();

    for control_set in control_sets {
        let key_path = [control_set.as_str(), "Control", "Windows"];
        let shutdown_time = match hive.lookup_value(&key_path, "ShutdownTime") {
            Ok(Some(RegistryValue::Binary(data))) if data.len() >= 8 => {
                let filetime = u64::from_le_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]);
                windows_filetime_to_rfc3339(filetime)
            }
            Ok(Some(RegistryValue::Qword(filetime))) => windows_filetime_to_rfc3339(filetime),
            Ok(Some(other)) => {
                warnings.push(format!(
                    "{}\\ShutdownTime has unsupported type: {:?}",
                    key_path.join("\\"),
                    other
                ));
                None
            }
            Ok(None) => {
                warnings.push(format!("{}\\ShutdownTime not found", key_path.join("\\")));
                None
            }
            Err(err) => {
                warnings.push(format!(
                    "{}\\ShutdownTime parse error: {}",
                    key_path.join("\\"),
                    err
                ));
                None
            }
        };

        if let Some(shutdown_time) = shutdown_time {
            entries.push(ShutdownTimeEntry {
                key_path: key_path.join("\\"),
                shutdown_time,
            });
        }
    }

    Ok(entries)
}
