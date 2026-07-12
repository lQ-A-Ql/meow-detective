use super::super::{first_string_attr, string_array_attr, CorrelationRuleMatch};
use super::{
    basename, dedup_rule_matches, extract_file_name_candidates, extract_path_candidates,
    find_best_file_by_name, find_best_file_by_path,
};
use domain::FileEntry;
use transport::dto::{ArtifactRowDto, CorrelationConfidenceDto, CorrelationEdgeKindDto};

// ── Rule match builders ──

pub(crate) fn build_artifact_rule_matches(
    files: &[FileEntry],
    artifact: &ArtifactRowDto,
) -> Vec<CorrelationRuleMatch> {
    match artifact.artifact_type.as_str() {
        "LNK" => build_single_path_rule(
            files,
            artifact,
            first_string_attr(&artifact.attrs, &["target_path", "targetPath"]),
            "LNK 目标路径命中文件路径",
            CorrelationEdgeKindDto::PathMatch,
            CorrelationConfidenceDto::Direct,
            "路径类匹配依赖工件字段规范化，必要时需回跳原始 LNK 字段复核。",
            None,
        ),
        "BrowserDownload" => build_single_path_rule(
            files,
            artifact,
            first_string_attr(&artifact.attrs, &["targetPath"]),
            "浏览器下载目标路径命中文件路径",
            CorrelationEdgeKindDto::PathMatch,
            CorrelationConfidenceDto::Direct,
            "下载路径来自浏览器数据库记录，仍需结合文件内容与时间线复核。",
            None,
        ),
        "BrowserHistory" => build_browser_history_rules(files, artifact),
        "RecycleBin" => build_single_path_rule(
            files,
            artifact,
            first_string_attr(&artifact.attrs, &["original_path", "originalPath"]),
            "Recycle Bin 原路径命中已删除文件",
            CorrelationEdgeKindDto::RecoveredOriginalPath,
            CorrelationConfidenceDto::Direct,
            "回收站原路径反映删除前路径声明，需结合 deleted 文件与删除时间复核。",
            Some(true),
        ),
        "RegistryValue" => build_registry_rules(files, artifact),
        "Prefetch" => build_name_rules(
            files,
            artifact,
            first_string_attr(&artifact.attrs, &["executable"])
                .map(|value| vec![basename(&value)])
                .unwrap_or_default(),
            "Prefetch 可执行名命中文件名",
            CorrelationConfidenceDto::Strong,
            "名称匹配可能命中同名文件，需要结合路径与时间进一步复核。",
        ),
        "EmailMessage" => build_name_rules(
            files,
            artifact,
            string_array_attr(&artifact.attrs, "attachments")
                .into_iter()
                .map(|value| basename(&value))
                .collect(),
            "邮件附件名命中文件名",
            CorrelationConfidenceDto::Weak,
            "附件名匹配只提供弱线索，需要结合时间、路径与邮件上下文复核。",
        )
        .into_iter()
        .chain(build_email_subject_rules(files, artifact))
        .collect(),
        "JumpList" => build_single_path_rule(
            files,
            artifact,
            first_string_attr(&artifact.attrs, &["target_path", "targetPath"]),
            "JumpList 目标路径命中文件路径",
            CorrelationEdgeKindDto::PathMatch,
            CorrelationConfidenceDto::Direct,
            "JumpList 命中依赖嵌入式 LNK 提取结果，需结合原始 JumpList 复核。",
            None,
        ),
        _ => Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_single_path_rule(
    files: &[FileEntry],
    artifact: &ArtifactRowDto,
    path: Option<String>,
    summary: &str,
    kind: CorrelationEdgeKindDto,
    confidence: CorrelationConfidenceDto,
    caveat: &str,
    prefer_deleted: Option<bool>,
) -> Vec<CorrelationRuleMatch> {
    let Some(path) = path else {
        return Vec::new();
    };
    let Some(file) = find_best_file_by_path(files, &path, prefer_deleted) else {
        return Vec::new();
    };
    vec![CorrelationRuleMatch {
        artifact: artifact.clone(),
        file: file.clone(),
        kind,
        confidence,
        summary: summary.to_string(),
        caveat: caveat.to_string(),
    }]
}

pub(crate) fn build_registry_rules(
    files: &[FileEntry],
    artifact: &ArtifactRowDto,
) -> Vec<CorrelationRuleMatch> {
    let mut matches = Vec::new();
    let Some(data) = first_string_attr(&artifact.attrs, &["data"]) else {
        return matches;
    };

    for path in extract_path_candidates(&data).into_iter().take(2) {
        if let Some(file) = find_best_file_by_path(files, &path, None) {
            matches.push(CorrelationRuleMatch {
                artifact: artifact.clone(),
                file: file.clone(),
                kind: CorrelationEdgeKindDto::PathMatch,
                confidence: CorrelationConfidenceDto::Strong,
                summary: "Registry 值数据命中文件路径".to_string(),
                caveat: "Registry 值可能包含环境变量或启动参数，命中后仍需回跳原始值复核。"
                    .to_string(),
            });
        }
    }

    if matches.is_empty() {
        let names = extract_file_name_candidates(&data);
        matches.extend(build_name_rules(
            files,
            artifact,
            names,
            "Registry 值数据命中文件名",
            CorrelationConfidenceDto::Weak,
            "Registry 名称匹配可能存在同名文件，需要结合路径与 key path 复核。",
        ));
    }

    dedup_rule_matches(&mut matches);
    matches
}

pub(crate) fn build_browser_history_rules(
    files: &[FileEntry],
    artifact: &ArtifactRowDto,
) -> Vec<CorrelationRuleMatch> {
    let mut names = Vec::new();
    if let Some(title) = first_string_attr(&artifact.attrs, &["title"]) {
        names.extend(extract_file_name_candidates(&title));
    }
    if let Some(url) = first_string_attr(&artifact.attrs, &["url"]) {
        names.extend(extract_file_name_candidates(&url));
    }

    build_name_rules(
        files,
        artifact,
        names,
        "BrowserHistory 标题或 URL 命中文件名",
        CorrelationConfidenceDto::Weak,
        "BrowserHistory 命中基于标题或 URL 文本，需要结合访问时间与原始记录复核。",
    )
}

pub(crate) fn build_name_rules(
    files: &[FileEntry],
    artifact: &ArtifactRowDto,
    names: Vec<String>,
    summary: &str,
    confidence: CorrelationConfidenceDto,
    caveat: &str,
) -> Vec<CorrelationRuleMatch> {
    let mut matches = Vec::new();
    for name in names {
        let Some(file) = find_best_file_by_name(files, &name, None) else {
            continue;
        };
        matches.push(CorrelationRuleMatch {
            artifact: artifact.clone(),
            file: file.clone(),
            kind: CorrelationEdgeKindDto::NameMatch,
            confidence: confidence.clone(),
            summary: summary.to_string(),
            caveat: caveat.to_string(),
        });
    }
    dedup_rule_matches(&mut matches);
    matches
}

pub(crate) fn build_email_subject_rules(
    files: &[FileEntry],
    artifact: &ArtifactRowDto,
) -> Vec<CorrelationRuleMatch> {
    let Some(subject) = first_string_attr(&artifact.attrs, &["subject"]) else {
        return Vec::new();
    };

    let names = extract_file_name_candidates(&subject);
    if names.is_empty() {
        return Vec::new();
    }

    build_name_rules(
        files,
        artifact,
        names,
        "邮件主题命中文件名",
        CorrelationConfidenceDto::Weak,
        "主题命名匹配只提供弱线索，需要结合 sentAt 与附件/时间线复核。",
    )
}
