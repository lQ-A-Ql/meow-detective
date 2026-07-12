#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegistryValue {
    String(String),
    Dword(u32),
    Qword(u64),
    MultiString(Vec<String>),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone)]
pub(crate) struct NkRecord {
    pub(crate) name: String,
    pub(crate) last_write_time: Option<u64>,
    pub(crate) num_subkeys: u32,
    pub(crate) subkeys_list_offset: u32,
    pub(crate) num_values: u32,
    pub(crate) values_list_offset: u32,
}
