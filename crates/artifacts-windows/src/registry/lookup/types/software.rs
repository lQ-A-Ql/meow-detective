#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstalledSoftwareInfo {
    pub display_name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub install_date: Option<String>,
    pub estimated_size_kb: Option<u64>,
    pub uninstall_string: Option<String>,
    pub source_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkProfileEntry {
    pub profile_guid: String,
    pub profile_name: String,
    pub description: Option<String>,
    pub date_created: Option<String>,
    pub date_last_connected: Option<String>,
    pub name_type: Option<u32>,
    pub managed: bool,
    pub first_network: Option<String>,
    pub default_gateway_mac_hex: Option<String>,
    pub dns_suffix: Option<String>,
    pub source_key_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppCompatLayerEntry {
    pub executable_path: String,
    pub layer_string: String,
    pub source_hive_path: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmcacheApplicationEntry {
    pub program_id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub install_date: Option<String>,
    pub source: Option<String>,
    pub os_version_at_install_time: Option<String>,
    pub registry_key_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmcacheApplicationFileEntry {
    pub program_id: Option<String>,
    pub lower_case_long_path: Option<String>,
    pub long_path_hash: Option<String>,
    pub file_size: Option<u64>,
    pub product_name: Option<String>,
    pub company_name: Option<String>,
    pub file_version: Option<String>,
    pub is_pe_file: Option<bool>,
    pub link_date: Option<String>,
    pub registry_key_path: String,
}
