pub mod amcache;
mod fields;
pub mod lsa;
pub mod muicache;
pub mod ntuser;
pub(crate) mod reader;
pub mod sam;
pub mod security;
pub mod shellbags;
pub mod software;
pub mod system;
mod time;
pub(crate) mod txlog_util;
pub mod types;
pub(crate) mod utf16;
mod value;

pub use amcache::{extract_amcache_entries, AmcacheInfo};
pub use muicache::extract_muicache_from_usrclass_hive;
pub use ntuser::{
    extract_appcompat_layers_from_ntuser_hive, extract_ntuser_fields,
    extract_ntuser_fields_with_txlog,
};
pub(crate) use reader::RegistryHiveReader;
pub use sam::{extract_sam_fields, extract_sam_fields_with_txlog};
pub use security::{
    extract_cached_credentials_from_security_hive, extract_lsa_secrets_from_security_hive,
    extract_security_policy_from_security_hive,
    extract_security_policy_from_security_hive_with_txlog,
};
pub use shellbags::extract_shellbags_from_usrclass_hive;
pub use software::{
    extract_appcompat_layers_from_software_hive, extract_installed_software,
    extract_machine_run_keys_from_software_hive, extract_network_profiles_from_software_hive,
    extract_software_hive_fields, extract_software_hive_fields_with_txlog,
    extract_winlogon_fields_from_software_hive,
};
pub use system::{
    extract_lsa_packages_from_system_hive, extract_mounted_devices_from_system_hive,
    extract_network_adapters_from_system_hive, extract_services_from_system_hive,
    extract_shimcache_from_system_hive, extract_shutdown_time_from_system_hive,
    extract_system_hive_fields, extract_system_hive_fields_with_txlog,
    extract_usb_devices_from_system_hive,
};
pub use types::*;

pub(crate) use crate::registry::txlog::parse_transaction_log;
pub(crate) use fields::{
    lookup_install_date_field, lookup_optional_dword_field, lookup_optional_string_field,
    lookup_string_field,
};
pub(crate) use time::{
    extract_utf16le_from_binary, filetime_to_utc, rot13_decode, windows_filetime_to_rfc3339,
};
pub(crate) use value::parse_value_data;

#[cfg(test)]
#[path = "../../../tests/unit/registry/lookup.rs"]
mod tests;
