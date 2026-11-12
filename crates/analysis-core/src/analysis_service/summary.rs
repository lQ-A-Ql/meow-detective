use transport::dto::{
    AnalysisFileClassificationDto, AnalysisParseStatusDto, AnalysisSystemInfoDto,
};

/// Generate analysis summary.
pub fn generate_analysis_summary(
    system_info: &AnalysisSystemInfoDto,
    classifications: &[AnalysisFileClassificationDto],
) -> String {
    let mut summary = String::new();

    summary.push_str("# 数据源分析报告\n\n");

    summary.push_str("## 系统信息\n\n");
    match system_info.status {
        AnalysisParseStatusDto::Parsed => {
            push_optional_line(&mut summary, "计算机名", &system_info.computer_name);
            push_optional_line(&mut summary, "操作系统", &system_info.os_version);
            push_optional_line(&mut summary, "Build 号", &system_info.build_number);
            push_optional_line(&mut summary, "注册用户", &system_info.registered_owner);
            push_optional_line(&mut summary, "时区", &system_info.timezone);
        }
        AnalysisParseStatusDto::Partial => {
            summary.push_str("- **状态**: 部分解析\n");
            push_optional_line(&mut summary, "计算机名", &system_info.computer_name);
            push_optional_line(&mut summary, "操作系统", &system_info.os_version);
            push_optional_line(&mut summary, "Build 号", &system_info.build_number);
            push_optional_line(&mut summary, "注册用户", &system_info.registered_owner);
            push_optional_line(&mut summary, "时区", &system_info.timezone);
        }
        AnalysisParseStatusDto::NotParsed => {
            summary.push_str("- **状态**: 未解析\n");
        }
        AnalysisParseStatusDto::Unavailable => {
            summary.push_str("- **状态**: 不可用\n");
        }
        AnalysisParseStatusDto::CandidateFound => {
            summary.push_str("- **状态**: 已发现候选\n");
        }
        AnalysisParseStatusDto::NotFound => {
            summary.push_str("- **状态**: 未发现\n");
        }
        AnalysisParseStatusDto::Failed => {
            summary.push_str("- **状态**: 解析失败\n");
        }
    }

    if !system_info.warnings.is_empty() {
        summary.push_str("\n### 系统信息告警\n\n");
        for warning in &system_info.warnings {
            summary.push_str(&format!("- {}\n", warning));
        }
    }

    if !system_info.network_adapters.is_empty() {
        summary.push_str("\n## 网络适配器\n\n");
        for adapter in &system_info.network_adapters {
            summary.push_str(&format!("- **{}**", adapter.name));
            if let Some(mac) = &adapter.mac_address {
                summary.push_str(&format!(" (MAC: {})", mac));
            }
            summary.push('\n');
        }
    }

    if !system_info.boot_history.is_empty() {
        summary.push_str("\n## 开关机历史\n\n");
        for boot in &system_info.boot_history {
            summary.push_str(&format!("- {} ({})\n", boot.timestamp, boot.boot_type));
        }
    }

    if !classifications.is_empty() {
        summary.push_str("\n## 文件分类\n\n");
        summary.push_str("| 类别 | 文件数 | 总大小 | 状态 |\n");
        summary.push_str("|------|--------|--------|------|\n");
        for cat in classifications {
            summary.push_str(&format!(
                "| {} | {} | {:.1} MB | {} |\n",
                cat.category,
                cat.file_count,
                cat.total_size as f64 / (1024.0 * 1024.0),
                status_label(&cat.status),
            ));
        }

        let warnings = classifications
            .iter()
            .flat_map(|cat| cat.warnings.iter())
            .collect::<Vec<_>>();
        if !warnings.is_empty() {
            summary.push_str("\n### 文件分类告警\n\n");
            for warning in warnings {
                summary.push_str(&format!("- {}\n", warning));
            }
        }
    } else {
        summary.push_str("\n## 文件分类\n\n- **状态**: 未发现可分类文件。\n");
    }

    summary
}

fn push_optional_line(summary: &mut String, label: &str, value: &Option<String>) {
    if let Some(value) = value {
        summary.push_str(&format!("- **{}**: {}\n", label, value));
    }
}

fn status_label(status: &AnalysisParseStatusDto) -> &'static str {
    match status {
        AnalysisParseStatusDto::Parsed => "已解析",
        AnalysisParseStatusDto::Partial => "部分解析",
        AnalysisParseStatusDto::NotParsed => "未解析",
        AnalysisParseStatusDto::Unavailable => "不可用",
        AnalysisParseStatusDto::CandidateFound => "已发现候选",
        AnalysisParseStatusDto::NotFound => "未发现",
        AnalysisParseStatusDto::Failed => "解析失败",
    }
}
