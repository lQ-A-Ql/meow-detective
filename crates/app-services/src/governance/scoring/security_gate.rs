use transport::dto::{ReleaseGateStatusDto, SecurityAuditSummaryDto};

use super::gate_status::GateResult;

pub(super) fn security_gate(security: &SecurityAuditSummaryDto) -> GateResult {
    let mut failed_controls = Vec::new();

    if security.export_overwrite_default {
        failed_controls.push("exportOverwriteDefault");
    }
    if !security.export_path_guard_enabled {
        failed_controls.push("exportPathGuardEnabled");
    }
    if !security.stdio_command_whitelist_enforced {
        failed_controls.push("stdioCommandWhitelistEnforced");
    }
    if !security.sse_https_only {
        failed_controls.push("sseHttpsOnly");
    }
    if !security.embedded_credentials_blocked {
        failed_controls.push("embeddedCredentialsBlocked");
    }
    if !security.media_handle_scoped {
        failed_controls.push("mediaHandleScoped");
    }
    if !security.error_redaction_enabled {
        failed_controls.push("errorRedactionEnabled");
    }
    if !security.audit_log_required || security.audit_event_count == 0 {
        failed_controls.push("auditLogRequired");
    }

    let status = if failed_controls.is_empty() {
        ReleaseGateStatusDto::Passed
    } else {
        ReleaseGateStatusDto::Blocked
    };
    let evidence = format!(
        "pathGuard={}, stdioWhitelist={}, sseHttpsOnly={}, embeddedCredentialsBlocked={}, mediaHandleScoped={}, errorRedactionEnabled={}, auditLogRequired={}, overwriteDefault={}",
        security.export_path_guard_enabled,
        security.stdio_command_whitelist_enforced,
        security.sse_https_only,
        security.embedded_credentials_blocked,
        security.media_handle_scoped,
        security.error_redaction_enabled,
        security.audit_log_required,
        security.export_overwrite_default
    );
    let detail = if failed_controls.is_empty() {
        "导出路径、防覆盖、MCP、媒体句柄与错误脱敏基线均已开启".to_string()
    } else {
        format!("安全基线缺失控件：{}", failed_controls.join("、"))
    };

    (status, evidence, detail)
}
