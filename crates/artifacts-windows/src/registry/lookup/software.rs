mod appcompat;
mod fields;
mod installed;
mod network;
mod startup;
mod values;

pub use appcompat::extract_appcompat_layers_from_software_hive;
pub use fields::{extract_software_hive_fields, extract_software_hive_fields_with_txlog};
pub use installed::extract_installed_software;
pub use network::extract_network_profiles_from_software_hive;
pub use startup::{
    extract_machine_run_keys_from_software_hive, extract_winlogon_fields_from_software_hive,
};

#[cfg(test)]
#[path = "../../../tests/unit/registry/software.rs"]
mod tests;
