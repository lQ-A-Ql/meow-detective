use super::{
    session::with_session, RegistryBrowserContext, RegistryBrowserError, RegistryBrowserRegistry,
};
use artifacts_windows::registry::browser::BrowserError;
use transport::dto::{
    RegistryHardwareDeviceDto, RegistryHardwareRequestDto, RegistryHardwareSummaryDto,
};

/// Hardware is read on demand from the same bounded base/recovered session.
pub fn read_hardware(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistryHardwareRequestDto,
) -> Result<RegistryHardwareSummaryDto, RegistryBrowserError> {
    with_session(registry, ctx, &request.session_id, |session| {
        let info = session
            .hive
            .hardware_inventory()
            .map_err(|_| BrowserError::Corrupt)?;
        Ok(RegistryHardwareSummaryDto {
            source: session.source.clone(),
            devices: info
                .devices
                .into_iter()
                .map(|record| RegistryHardwareDeviceDto {
                    kind: record.kind,
                    device_id: record.device_id,
                    name: record.name,
                    manufacturer: record.manufacturer,
                    properties: record.properties,
                    key_path: record.key_path,
                    value_name: record.value_name,
                    source_offset: record.source_offset,
                })
                .collect(),
            warnings: info.warnings,
        })
    })
}
