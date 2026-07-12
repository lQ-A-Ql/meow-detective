use super::super::{dedup_vec, CorrelationRuleMatch, CorrelationSourceGroup};
use std::collections::BTreeSet;
use transport::dto::{
    CorrelationConfidenceDto, CorrelationEdgeKindDto, CorrelationJumpTargetDto,
    VerificationGuaranteeLevelDto,
};

pub(crate) fn build_lead_jumps(group: &CorrelationSourceGroup) -> Vec<CorrelationJumpTargetDto> {
    let mut jumps = vec![CorrelationJumpTargetDto {
        route: "/files".to_string(),
        target_id: group.source_object_id.clone(),
        label: "查看文件".to_string(),
    }];
    if let Some(artifact) = group.artifacts.first() {
        jumps.push(CorrelationJumpTargetDto {
            route: "/artifacts".to_string(),
            target_id: artifact.id.clone(),
            label: "查看痕迹".to_string(),
        });
    }
    if let Some(timeline) = group.timelines.first() {
        jumps.push(CorrelationJumpTargetDto {
            route: "/timeline".to_string(),
            target_id: timeline.id.clone(),
            label: "查看时间线".to_string(),
        });
    }
    jumps
}

pub(crate) fn group_title(group: &CorrelationSourceGroup) -> String {
    group
        .file
        .as_ref()
        .map(|file| file.name.clone())
        .unwrap_or_else(|| group.source_object_id.clone())
}

pub(crate) fn group_summary(group: &CorrelationSourceGroup) -> String {
    let artifact_count = group.artifacts.len();
    let timeline_count = group.timelines.len();
    match (artifact_count, timeline_count) {
        (0, timeline_count) => format!("同一 source object 命中 {timeline_count} 条时间线事件。"),
        (artifact_count, 0) => format!("同一 source object 命中 {artifact_count} 条痕迹记录。"),
        (artifact_count, timeline_count) => {
            format!(
                "同一 source object 聚合 {artifact_count} 条痕迹记录与 {timeline_count} 条时间线事件。"
            )
        }
    }
}

pub(crate) fn group_caveats(
    group: &CorrelationSourceGroup,
    artifact_count: u32,
    timeline_count: u32,
) -> Vec<String> {
    let mut caveats = Vec::new();
    if group.file.is_none() {
        caveats.push(
            "source_object_id 未映射到 file_entries，需结合原始工件与导入链路复核。".to_string(),
        );
    }
    if artifact_count == 0 || timeline_count == 0 {
        caveats.push("当前仅形成单侧证据命中，尚未完成跨工件交叉验证。".to_string());
    }
    if timeline_count > 0 {
        caveats.push("时间线命中可能来自聚合投影，解释时需回跳原始事件。".to_string());
    }
    caveats
}

pub(crate) fn source_group_match_signals(group: &CorrelationSourceGroup) -> Vec<String> {
    let mut signals = Vec::new();
    if !group.artifacts.is_empty() {
        signals.push(format!(
            "同一 source object 命中 {} 条 artifact",
            group.artifacts.len()
        ));
    }
    if !group.timelines.is_empty() {
        signals.push(format!(
            "同一 source object 命中 {} 条 timeline",
            group.timelines.len()
        ));
    }
    signals
}

pub(crate) fn rule_group_match_signals(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    timeline_signals: &[String],
) -> Vec<String> {
    let mut signals = matches
        .iter()
        .map(|item| item.summary.clone())
        .collect::<Vec<_>>();
    if timeline_count > 0 {
        signals.push("关联文件时间线事件提供上下文".to_string());
    }
    signals.extend(timeline_signals.iter().cloned());
    dedup_vec(&mut signals);
    signals
}

pub(crate) fn rule_group_summary(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    proximity_timeline_count: u32,
) -> String {
    let (path_matches, name_matches, recovered_matches, artifact_types) =
        summarize_rule_matches(matches);
    format!(
        "{} 规则命中 {} 条记录（路径 {}，名称 {}，原路径恢复 {}，自身时间线 {}，邻近时间线 {}）。",
        artifact_types.into_iter().collect::<Vec<_>>().join(" / "),
        matches.len(),
        path_matches,
        name_matches,
        recovered_matches,
        timeline_count,
        proximity_timeline_count
    )
}

fn summarize_rule_matches(
    matches: &[CorrelationRuleMatch],
) -> (usize, usize, usize, BTreeSet<String>) {
    let mut path_matches = 0;
    let mut name_matches = 0;
    let mut recovered_matches = 0;
    let mut artifact_types = BTreeSet::new();
    for item in matches {
        artifact_types.insert(item.artifact.artifact_type.clone());
        match item.kind {
            CorrelationEdgeKindDto::PathMatch => path_matches += 1,
            CorrelationEdgeKindDto::NameMatch => name_matches += 1,
            CorrelationEdgeKindDto::RecoveredOriginalPath => recovered_matches += 1,
            _ => {}
        }
    }
    (
        path_matches,
        name_matches,
        recovered_matches,
        artifact_types,
    )
}

pub(crate) fn rule_group_caveats(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    has_proximity_timeline: bool,
) -> Vec<String> {
    let mut caveats = matches
        .iter()
        .map(|item| item.caveat.clone())
        .collect::<Vec<_>>();
    if timeline_count == 0 && !has_proximity_timeline {
        caveats.push("当前规则命中尚未获得同文件时间线佐证。".to_string());
    }
    dedup_vec(&mut caveats);
    caveats
}

pub(crate) fn artifact_guarantee_level(artifact_type: &str) -> VerificationGuaranteeLevelDto {
    match artifact_type {
        "Prefetch" | "LNK" | "Registry" | "RegistryValue" | "RecycleBin" => {
            VerificationGuaranteeLevelDto::BestEffort
        }
        _ => VerificationGuaranteeLevelDto::Experimental,
    }
}

pub(crate) fn timeline_guarantee_level(parser_id: Option<&str>) -> VerificationGuaranteeLevelDto {
    match parser_id {
        Some(parser_id) if parser_id.starts_with("timeline.") || parser_id.starts_with("evtx.") => {
            VerificationGuaranteeLevelDto::BestEffort
        }
        _ => VerificationGuaranteeLevelDto::Experimental,
    }
}

pub(crate) fn group_confidence(
    artifact_count: u32,
    timeline_count: u32,
) -> CorrelationConfidenceDto {
    if artifact_count > 0 && timeline_count > 0 {
        CorrelationConfidenceDto::Direct
    } else if artifact_count + timeline_count >= 3 {
        CorrelationConfidenceDto::Strong
    } else if artifact_count + timeline_count >= 1 {
        CorrelationConfidenceDto::Weak
    } else {
        CorrelationConfidenceDto::Heuristic
    }
}

pub(crate) fn rule_group_confidence(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    has_proximity_timeline: bool,
) -> CorrelationConfidenceDto {
    if matches.iter().any(|item| {
        matches!(
            item.kind,
            CorrelationEdgeKindDto::RecoveredOriginalPath | CorrelationEdgeKindDto::PathMatch
        )
    }) {
        CorrelationConfidenceDto::Direct
    } else if ((timeline_count > 0 || has_proximity_timeline) && !matches.is_empty())
        || matches.len() >= 2
        || matches
            .iter()
            .any(|item| item.confidence == CorrelationConfidenceDto::Strong)
    {
        CorrelationConfidenceDto::Strong
    } else if !matches.is_empty() {
        CorrelationConfidenceDto::Weak
    } else {
        CorrelationConfidenceDto::Heuristic
    }
}
