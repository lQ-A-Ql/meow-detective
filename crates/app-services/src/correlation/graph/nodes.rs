use super::super::CorrelationSourceGroup;
use super::{artifact_guarantee_level, timeline_guarantee_level};
use domain::FileEntry;
use transport::dto::{
    ArtifactRowDto, CorrelationJumpTargetDto, CorrelationNodeDto, CorrelationNodeKindDto,
    CorrelationProvenanceDto, TimelineEventDto,
};

pub(crate) fn build_file_node(
    file_node_id: &str,
    group: &CorrelationSourceGroup,
    related_count: u32,
) -> CorrelationNodeDto {
    if let Some(file) = &group.file {
        return build_file_node_for_entry(file_node_id, file, related_count);
    }
    CorrelationNodeDto {
        id: file_node_id.to_string(),
        kind: CorrelationNodeKindDto::File,
        title: group.source_object_id.clone(),
        subtitle: Some("未能映射到 file_entries，需回查原始工件。".to_string()),
        source_object_id: Some(group.source_object_id.clone()),
        related_count,
        badges: vec!["unresolved".to_string()],
        jumps: vec![CorrelationJumpTargetDto {
            route: "/files".to_string(),
            target_id: group.source_object_id.clone(),
            label: "打开文件浏览".to_string(),
        }],
    }
}

pub(crate) fn build_file_node_for_entry(
    file_node_id: &str,
    file: &FileEntry,
    related_count: u32,
) -> CorrelationNodeDto {
    let title = if file.entry_type == domain::EntryType::Directory {
        format!("{}/", file.name)
    } else {
        file.name.clone()
    };
    CorrelationNodeDto {
        id: file_node_id.to_string(),
        kind: CorrelationNodeKindDto::File,
        title,
        subtitle: Some(file.path.clone()),
        source_object_id: Some(file.id.0.clone()),
        related_count,
        badges: file_badges(file),
        jumps: vec![CorrelationJumpTargetDto {
            route: "/files".to_string(),
            target_id: file.id.0.clone(),
            label: "打开文件浏览".to_string(),
        }],
    }
}

fn file_badges(file: &FileEntry) -> Vec<String> {
    let mut badges = Vec::new();
    if file.deleted {
        badges.push("deleted".to_string());
    }
    if file.hidden {
        badges.push("hidden".to_string());
    }
    if file.system {
        badges.push("system".to_string());
    }
    badges
}

pub(crate) fn build_artifact_node(
    artifact: &ArtifactRowDto,
    related_count: u32,
) -> CorrelationNodeDto {
    CorrelationNodeDto {
        id: format!("artifact:{}", artifact.id),
        kind: CorrelationNodeKindDto::Artifact,
        title: artifact.title.clone(),
        subtitle: Some(artifact.summary.clone()),
        source_object_id: artifact.source_object_id.clone(),
        related_count,
        badges: vec![artifact.artifact_type.clone()],
        jumps: vec![CorrelationJumpTargetDto {
            route: "/artifacts".to_string(),
            target_id: artifact.id.clone(),
            label: "打开痕迹分析".to_string(),
        }],
    }
}

pub(crate) fn build_timeline_node(
    timeline: &TimelineEventDto,
    related_count: u32,
) -> CorrelationNodeDto {
    CorrelationNodeDto {
        id: format!("timeline:{}", timeline.id),
        kind: CorrelationNodeKindDto::TimelineEvent,
        title: timeline.title.clone(),
        subtitle: Some(format!("{} · {}", timeline.ts, timeline.event_type)),
        source_object_id: Some(timeline.source_object_id.clone()),
        related_count,
        badges: vec![timeline.event_type.clone()],
        jumps: vec![CorrelationJumpTargetDto {
            route: "/timeline".to_string(),
            target_id: timeline.id.clone(),
            label: "打开时间线".to_string(),
        }],
    }
}

pub(crate) fn build_artifact_provenance(artifact: &ArtifactRowDto) -> CorrelationProvenanceDto {
    CorrelationProvenanceDto {
        source_kind: "artifact".to_string(),
        source_record_id: artifact.id.clone(),
        source_label: artifact.artifact_type.clone(),
        producer: artifact.extractor_id.clone(),
        producer_version: artifact.extractor_version.clone(),
        guarantee_level: artifact_guarantee_level(&artifact.artifact_type),
        warning_summary: Vec::new(),
    }
}

pub(crate) fn build_timeline_provenance(timeline: &TimelineEventDto) -> CorrelationProvenanceDto {
    CorrelationProvenanceDto {
        source_kind: "timeline".to_string(),
        source_record_id: timeline.id.clone(),
        source_label: timeline.event_type.clone(),
        producer: timeline.parser_id.clone(),
        producer_version: timeline.parser_version.clone(),
        guarantee_level: timeline_guarantee_level(timeline.parser_id.as_deref()),
        warning_summary: Vec::new(),
    }
}
