use app_services::{import_analysis, import_precheck};
use domain::DataSourcePlatform;
use transport::{
    commands::{AppSettingsDto, ImportDataSourceRequest, ImportTargetPlatformDto},
    CommandError,
};

pub(super) fn load_import_settings(path: &std::path::Path) -> AppSettingsDto {
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<AppSettingsDto>(&raw) {
            Ok(settings) => {
                if let Err(error) = settings.validate() {
                    tracing::warn!(
                        "Ignoring invalid app settings at {}: {}",
                        path.display(),
                        error
                    );
                    AppSettingsDto::default()
                } else {
                    settings
                }
            }
            Err(error) => {
                tracing::warn!(
                    "Ignoring unreadable app settings at {}: {}",
                    path.display(),
                    error
                );
                AppSettingsDto::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppSettingsDto::default(),
        Err(error) => {
            tracing::warn!(
                "Ignoring app settings read error at {}: {}",
                path.display(),
                error
            );
            AppSettingsDto::default()
        }
    }
}

pub(super) fn import_analysis_mode_from_settings(
    value: &str,
) -> import_analysis::ImportAnalysisMode {
    match value {
        "budgetedContent" => import_analysis::ImportAnalysisMode::BudgetedContent,
        "fullContent" => import_analysis::ImportAnalysisMode::FullContent,
        _ => import_analysis::ImportAnalysisMode::MetadataOnly,
    }
}

pub(super) fn prepare_import_config(
    request: &ImportDataSourceRequest,
    platform: DataSourcePlatform,
) -> Result<import_precheck::ImportSourceConfig, CommandError> {
    import_precheck::prepare_import_source_config(
        &request.source_path,
        platform,
        request.profile.clone(),
    )
    .map_err(super::import_config_error_to_command_error)
}

pub(super) fn import_platform_from_dto(
    platform: ImportTargetPlatformDto,
) -> Result<DataSourcePlatform, CommandError> {
    match platform {
        ImportTargetPlatformDto::Windows => Ok(DataSourcePlatform::Windows),
        ImportTargetPlatformDto::Linux => Ok(DataSourcePlatform::Linux),
        ImportTargetPlatformDto::Unsupported => Err(CommandError::unsupported(
            "unsupported data source platform; only Windows and Linux are supported",
        )),
    }
}

pub(super) fn validate_import_request(
    request: &ImportDataSourceRequest,
) -> Result<DataSourcePlatform, CommandError> {
    let platform = import_platform_from_dto(request.platform)?;
    request.validate().map_err(CommandError::invalid_input)?;
    Ok(platform)
}

pub(super) fn map_import_config_error(
    error: import_precheck::ImportSourceConfigError,
) -> CommandError {
    if error.is_invalid_input() {
        CommandError::invalid_input(error.to_string())
    } else {
        CommandError::from_typed_service_error(error)
    }
}
