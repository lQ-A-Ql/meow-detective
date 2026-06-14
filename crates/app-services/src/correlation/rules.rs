use super::{first_string_attr, path_suffix_key, string_array_attr, CorrelationRuleMatch};
use domain::FileEntry;
use std::collections::BTreeSet;
use transport::dto::{
    ArtifactRowDto, CorrelationConfidenceDto, CorrelationEdgeKindDto, TimelineEventDto,
};

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

// ── Rule helpers ──

pub(crate) fn dedup_rule_matches(matches: &mut Vec<CorrelationRuleMatch>) {
    let mut seen = BTreeSet::new();
    matches.retain(|item| {
        seen.insert((
            item.artifact.id.clone(),
            item.file.id.0.clone(),
            item.kind.clone(),
        ))
    });
}

pub(crate) fn find_best_file_by_path<'a>(
    files: &'a [FileEntry],
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a FileEntry> {
    let normalized = normalize_path(candidate);
    if normalized.is_empty() {
        return None;
    }

    let exact = files
        .iter()
        .filter(|file| file.is_file())
        .filter(|file| super::deleted_preference_score(file, prefer_deleted) < 2)
        .filter(|file| normalize_path(&file.path) == normalized)
        .min_by_key(|file| {
            (
                super::deleted_preference_score(file, prefer_deleted),
                file.path.len(),
            )
        });
    if exact.is_some() {
        return exact;
    }

    let suffix = path_suffix_key(&normalized);
    if suffix.is_empty() {
        return None;
    }

    files
        .iter()
        .filter(|file| file.is_file())
        .filter(|file| super::deleted_preference_score(file, prefer_deleted) < 2)
        .filter(|file| path_suffix_key(&file.path).ends_with(&suffix))
        .min_by_key(|file| {
            (
                super::deleted_preference_score(file, prefer_deleted),
                file.path.len(),
            )
        })
}

pub(crate) fn find_best_file_by_name<'a>(
    files: &'a [FileEntry],
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a FileEntry> {
    let normalized = basename(candidate);
    if normalized.is_empty() {
        return None;
    }

    files
        .iter()
        .filter(|file| file.is_file())
        .filter(|file| file.name.eq_ignore_ascii_case(&normalized))
        .filter(|file| super::deleted_preference_score(file, prefer_deleted) < 2)
        .min_by_key(|file| {
            (
                super::deleted_preference_score(file, prefer_deleted),
                file.path.len(),
            )
        })
}

// ── Path/normalize helpers ──

pub(crate) fn normalize_path(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']' | '<' | '>'));
    if trimmed.is_empty() {
        return String::new();
    }
    let mut normalized = trimmed.replace('\\', "/").to_ascii_lowercase();
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    while normalized.ends_with('/') && normalized.len() > 3 {
        normalized.pop();
    }
    normalized
}

pub(crate) fn basename(value: &str) -> String {
    normalize_path(value)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn looks_like_path(value: &str) -> bool {
    let candidate = value.trim();
    candidate.contains(":\\")
        || candidate.contains(":/")
        || candidate.starts_with("\\\\")
        || candidate.starts_with("//")
}

// ── Extraction helpers ──

pub(crate) fn extract_path_candidates(value: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let trimmed = value.trim();
    if looks_like_path(trimmed) {
        candidates.push(trimmed.to_string());
    }

    for segment in extract_quoted_segments(trimmed) {
        if looks_like_path(&segment) {
            candidates.push(segment);
        }
    }

    for token in trimmed.split(|ch: char| {
        ch.is_whitespace() || matches!(ch, ',' | ';' | '|' | '(' | ')' | '[' | ']')
    }) {
        if looks_like_path(token) {
            candidates.push(token.to_string());
        }
    }

    candidates
        .into_iter()
        .map(|item| {
            item.trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches('.')
                .trim_end_matches(',')
                .to_string()
        })
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn extract_file_name_candidates(value: &str) -> Vec<String> {
    let mut names = Vec::new();
    let trimmed = value.trim();
    let direct_name = basename(trimmed);
    if direct_name.contains('.') && !looks_like_path(trimmed) {
        names.push(direct_name);
    }

    for token in trimmed.split(|ch: char| {
        ch.is_whitespace() || matches!(ch, ',' | ';' | '|' | '(' | ')' | '[' | ']')
    }) {
        let name = basename(token);
        if name.contains('.') && !name.is_empty() {
            names.push(name);
        }
    }

    names
        .into_iter()
        .map(|item| {
            item.trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches('.')
                .trim_end_matches(',')
                .to_string()
        })
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn extract_quoted_segments(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in value.chars() {
        match quote {
            Some(active) if ch == active => {
                if !current.trim().is_empty() {
                    items.push(current.trim().to_string());
                }
                current.clear();
                quote = None;
            }
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None => {}
        }
    }

    items
}

// ── Rule match helpers ──

pub(crate) fn rule_match_timestamps(
    rule: &CorrelationRuleMatch,
) -> Vec<chrono::DateTime<chrono::Utc>> {
    match rule.artifact.artifact_type.as_str() {
        "BrowserHistory" => first_string_attr(&rule.artifact.attrs, &["visitTime"])
            .and_then(|value| super::parse_rfc3339_utc(&value))
            .into_iter()
            .collect(),
        "BrowserDownload" => first_string_attr(&rule.artifact.attrs, &["startTime"])
            .and_then(|value| super::parse_rfc3339_utc(&value))
            .into_iter()
            .collect(),
        "EmailMessage" => first_string_attr(&rule.artifact.attrs, &["sentAt"])
            .and_then(|value| super::parse_rfc3339_utc(&value))
            .into_iter()
            .collect(),
        "RecycleBin" => Vec::new(),
        _ => Vec::new(),
    }
}

pub(crate) fn rule_match_paths(rule: &CorrelationRuleMatch) -> Vec<String> {
    match rule.artifact.artifact_type.as_str() {
        "BrowserDownload" => first_string_attr(&rule.artifact.attrs, &["targetPath"])
            .into_iter()
            .collect(),
        "JumpList" | "LNK" => {
            first_string_attr(&rule.artifact.attrs, &["target_path", "targetPath"])
                .into_iter()
                .collect()
        }
        "RecycleBin" => first_string_attr(&rule.artifact.attrs, &["original_path", "originalPath"])
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn rule_match_text_needles(rule: &CorrelationRuleMatch) -> Vec<String> {
    match rule.artifact.artifact_type.as_str() {
        "BrowserHistory" => {
            let mut values = Vec::new();
            if let Some(url) = first_string_attr(&rule.artifact.attrs, &["url"]) {
                values.push(url);
            }
            if let Some(title) = first_string_attr(&rule.artifact.attrs, &["title"]) {
                values.push(title);
            }
            values
        }
        "EmailMessage" => {
            let mut values = Vec::new();
            if let Some(subject) = first_string_attr(&rule.artifact.attrs, &["subject"]) {
                values.push(subject);
            }
            values.extend(string_array_attr(&rule.artifact.attrs, "attachments"));
            values
        }
        _ => Vec::new(),
    }
}

// ── Timeline helpers ──

pub(crate) fn timeline_path_candidates(timeline: &TimelineEventDto) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(value) = first_string_attr(&timeline.attrs, &["path", "targetPath", "sourcePath"]) {
        candidates.push(value);
    }
    if let Some(value) = timeline.source_attribution.clone() {
        if looks_like_path(&value) {
            candidates.push(value);
        }
    }
    candidates
}

pub(crate) fn timeline_text_candidates(timeline: &TimelineEventDto) -> Vec<String> {
    let mut candidates = vec![timeline.title.clone(), timeline.description.clone()];
    if let Some(value) = first_string_attr(&timeline.attrs, &["url", "title"]) {
        candidates.push(value);
    }
    candidates
}
