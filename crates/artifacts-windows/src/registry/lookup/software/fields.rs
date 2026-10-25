use super::super::{
    lookup_install_date_field, lookup_optional_dword_field, lookup_optional_string_field,
    lookup_string_field, RegistryHiveReader, SoftwareHiveInfo,
};
use crate::registry::RegistryError;

pub fn extract_software_hive_fields(
    bytes: &(impl crate::registry::RegistryHiveSource + ?Sized),
    hive_path: &str,
) -> Result<SoftwareHiveInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key = ["Microsoft", "Windows NT", "CurrentVersion"];
    let mut info = SoftwareHiveInfo::default();

    info.product_name = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "ProductName",
        &mut info.warnings,
    );
    info.current_build = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "CurrentBuild",
        &mut info.warnings,
    );
    info.update_build_revision = lookup_optional_dword_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "UBR",
        &mut info.warnings,
    );
    info.current_version = lookup_optional_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "CurrentVersion",
        &mut info.warnings,
    );
    info.display_version = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "DisplayVersion",
        &mut info.warnings,
    )
    .or_else(|| {
        lookup_string_field(
            &hive,
            hive_path,
            "registry.software",
            &key,
            "ReleaseId",
            &mut info.warnings,
        )
    });
    info.registered_owner = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "RegisteredOwner",
        &mut info.warnings,
    );
    info.registered_organization = lookup_optional_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "RegisteredOrganization",
        &mut info.warnings,
    );
    info.product_id = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "ProductId",
        &mut info.warnings,
    );
    info.install_date = lookup_install_date_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        &mut info.warnings,
    );
    Ok(info)
}

pub fn extract_software_hive_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<SoftwareHiveInfo, RegistryError> {
    let snapshot = crate::registry::RegistrySnapshot::prepare(bytes, Some(txlog_data), None)?;
    let mut info = extract_software_hive_fields(&snapshot, hive_path)?;
    info.txlog_applied = snapshot.logs_applied();
    Ok(info)
}
