use super::super::txlog_util::apply_single_txlog_override;
use super::super::{
    lookup_install_date_field, lookup_optional_string_field, lookup_string_field,
    ParsedRegistryField, RegistryHiveReader, SoftwareHiveInfo, TxlogTimestampInfo,
};
use crate::registry::txlog::parse_transaction_log;
use crate::registry::RegistryError;

pub fn extract_software_hive_fields(
    bytes: &[u8],
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
    let mut info = extract_software_hive_fields(bytes, hive_path)?;
    let txlog = parse_transaction_log(txlog_data)?;
    let mut txlog_applied = false;
    let mut timestamps: Vec<TxlogTimestampInfo> = Vec::new();
    let fields: [&mut Option<ParsedRegistryField>; 8] = [
        &mut info.product_name,
        &mut info.current_build,
        &mut info.current_version,
        &mut info.display_version,
        &mut info.install_date,
        &mut info.registered_owner,
        &mut info.registered_organization,
        &mut info.product_id,
    ];
    for field in fields.into_iter().flatten() {
        let timestamp = apply_single_txlog_override(field, &txlog.transactions);
        txlog_applied |= timestamp.txlog_used;
        timestamps.push(timestamp);
    }
    info.txlog_applied = txlog_applied;
    info.txlog_timestamps = timestamps;
    Ok(info)
}
