#[derive(Debug)]
pub(super) struct AndroidFact {
    pub(super) field: &'static str,
    pub(super) value: String,
    pub(super) source_file_id: String,
    pub(super) source_path: String,
    pub(super) source_rank: i64,
    pub(super) confidence: &'static str,
    pub(super) warning: Option<String>,
}

#[derive(Debug)]
pub(super) struct AndroidPackage {
    pub(super) package_name: String,
    pub(super) version_code: Option<String>,
    pub(super) install_time: Option<String>,
    pub(super) update_time: Option<String>,
    pub(super) installer: Option<String>,
    pub(super) uid: Option<u32>,
    pub(super) code_path: Option<String>,
    pub(super) source_file_id: String,
    pub(super) source_path: String,
    pub(super) warning: Option<String>,
}
