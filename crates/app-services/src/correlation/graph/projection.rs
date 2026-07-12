use super::super::coverage::{derive_rule_group_families, derive_source_group_families};
use super::super::{
    dedup_vec, edge_kind_token, insert_node, CorrelationRuleGroup, CorrelationSourceGroup,
};
use super::{
    build_artifact_node, build_artifact_provenance, build_file_node, build_file_node_for_entry,
    build_lead_jumps, build_timeline_node, build_timeline_provenance, group_caveats,
    group_confidence, group_summary, group_title, rule_group_caveats, rule_group_confidence,
    rule_group_match_signals, rule_group_summary, source_group_match_signals,
};
use std::collections::BTreeMap;
use transport::dto::{
    CorrelationClusterDto, CorrelationEdgeDto, CorrelationEdgeKindDto, CorrelationJumpTargetDto,
    CorrelationLeadDto, CorrelationNodeDto, CorrelationProvenanceDto,
};

type NodeMap = BTreeMap<String, CorrelationNodeDto>;
type EdgeMap = BTreeMap<String, CorrelationEdgeDto>;

#[derive(Default)]
struct ProjectionParts {
    node_ids: Vec<String>,
    edge_ids: Vec<String>,
    supporting_node_ids: Vec<String>,
    provenance: Vec<CorrelationProvenanceDto>,
    jumps: Vec<CorrelationJumpTargetDto>,
}

impl ProjectionParts {
    fn extend(&mut self, other: ProjectionParts) {
        self.node_ids.extend(other.node_ids);
        self.edge_ids.extend(other.edge_ids);
        self.supporting_node_ids.extend(other.supporting_node_ids);
        self.provenance.extend(other.provenance);
        self.jumps.extend(other.jumps);
    }

    fn dedup(&mut self) {
        dedup_vec(&mut self.node_ids);
        dedup_vec(&mut self.edge_ids);
        dedup_vec(&mut self.supporting_node_ids);
        dedup_vec(&mut self.provenance);
        dedup_vec(&mut self.jumps);
    }
}

pub(crate) fn append_source_group(
    group: &CorrelationSourceGroup,
    node_map: &mut NodeMap,
    edge_map: &mut EdgeMap,
    clusters: &mut Vec<CorrelationClusterDto>,
    leads: &mut Vec<CorrelationLeadDto>,
) {
    let file_node_id = format!("file:{}", group.source_object_id);
    let artifact_count = group.artifacts.len() as u32;
    let timeline_count = group.timelines.len() as u32;
    let related_count = artifact_count + timeline_count;
    insert_node(
        node_map,
        build_file_node(&file_node_id, group, related_count),
    );
    let mut parts =
        project_source_artifacts(node_map, edge_map, group, &file_node_id, related_count);
    parts.extend(project_source_timelines(
        node_map,
        edge_map,
        group,
        &file_node_id,
        related_count,
        artifact_count,
    ));
    if let Some(edge_id) = insert_source_shared_edge(edge_map, group) {
        parts.edge_ids.push(edge_id);
    }
    push_source_results(
        group,
        artifact_count,
        timeline_count,
        parts,
        clusters,
        leads,
    );
}

fn project_source_artifacts(
    node_map: &mut NodeMap,
    edge_map: &mut EdgeMap,
    group: &CorrelationSourceGroup,
    file_node_id: &str,
    related_count: u32,
) -> ProjectionParts {
    let mut parts = ProjectionParts::default();
    for artifact in &group.artifacts {
        let node = build_artifact_node(artifact, related_count);
        let node_id = node.id.clone();
        let edge_id = format!("edge:{}:{}", artifact.id, group.source_object_id);
        insert_node(node_map, node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::SourceReference,
                from_node_id: node_id.clone(),
                to_node_id: file_node_id.to_string(),
                summary: format!("{} 引用同一 source object", artifact.artifact_type),
                confidence: transport::dto::CorrelationConfidenceDto::Direct,
            });
        parts.node_ids.push(node_id.clone());
        parts.edge_ids.push(edge_id);
        parts.supporting_node_ids.push(node_id);
        parts.provenance.push(build_artifact_provenance(artifact));
    }
    parts
}

fn project_source_timelines(
    node_map: &mut NodeMap,
    edge_map: &mut EdgeMap,
    group: &CorrelationSourceGroup,
    file_node_id: &str,
    related_count: u32,
    artifact_count: u32,
) -> ProjectionParts {
    let mut parts = ProjectionParts::default();
    for timeline in &group.timelines {
        let node = build_timeline_node(timeline, related_count);
        let node_id = node.id.clone();
        let edge_id = format!("edge:{}:{}", timeline.id, group.source_object_id);
        insert_node(node_map, node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::TemporalContext,
                from_node_id: node_id.clone(),
                to_node_id: file_node_id.to_string(),
                summary: format!("{} 时间线事件命中同一 source object", timeline.event_type),
                confidence: if artifact_count > 0 {
                    transport::dto::CorrelationConfidenceDto::Direct
                } else {
                    transport::dto::CorrelationConfidenceDto::Strong
                },
            });
        parts.node_ids.push(node_id.clone());
        parts.edge_ids.push(edge_id);
        parts.supporting_node_ids.push(node_id);
        parts.provenance.push(build_timeline_provenance(timeline));
    }
    parts
}

fn insert_source_shared_edge(
    edge_map: &mut EdgeMap,
    group: &CorrelationSourceGroup,
) -> Option<String> {
    let (Some(artifact), Some(timeline)) = (group.artifacts.first(), group.timelines.first())
    else {
        return None;
    };
    let edge_id = format!("edge:shared:{}:{}", artifact.id, timeline.id);
    edge_map
        .entry(edge_id.clone())
        .or_insert(CorrelationEdgeDto {
            id: edge_id.clone(),
            kind: CorrelationEdgeKindDto::SharedSourceObject,
            from_node_id: format!("artifact:{}", artifact.id),
            to_node_id: format!("timeline:{}", timeline.id),
            summary: "Artifact 与时间线共享同一 source object".to_string(),
            confidence: transport::dto::CorrelationConfidenceDto::Direct,
        });
    Some(edge_id)
}

fn push_source_results(
    group: &CorrelationSourceGroup,
    artifact_count: u32,
    timeline_count: u32,
    parts: ProjectionParts,
    clusters: &mut Vec<CorrelationClusterDto>,
    leads: &mut Vec<CorrelationLeadDto>,
) {
    let confidence = group_confidence(artifact_count, timeline_count);
    let title = group_title(group);
    let summary = group_summary(group);
    let families = derive_source_group_families(group);
    clusters.push(CorrelationClusterDto {
        id: format!("cluster:{}", group.source_object_id),
        title: title.clone(),
        summary: summary.clone(),
        confidence: confidence.clone(),
        families: families.clone(),
        primary_file_id: group.source_object_id.clone(),
        artifact_count,
        timeline_count,
        node_ids: parts.node_ids,
        edge_ids: parts.edge_ids,
        provenance: parts.provenance.clone(),
    });
    leads.push(CorrelationLeadDto {
        id: format!("lead:{}", group.source_object_id),
        title: format!("{title} 形成关联线索"),
        summary,
        confidence,
        families,
        primary_file_id: group.source_object_id.clone(),
        supporting_node_ids: parts.supporting_node_ids,
        match_signals: source_group_match_signals(group),
        jumps: build_lead_jumps(group),
        provenance: parts.provenance,
        caveats: group_caveats(group, artifact_count, timeline_count),
    });
}

pub(crate) fn append_rule_group(
    group: &CorrelationRuleGroup,
    node_map: &mut NodeMap,
    edge_map: &mut EdgeMap,
    clusters: &mut Vec<CorrelationClusterDto>,
    leads: &mut Vec<CorrelationLeadDto>,
) {
    let file_node_id = format!("file:{}", group.file.id.0);
    let related_count = (group.matches.len() + group.timelines.len()) as u32;
    insert_node(
        node_map,
        build_file_node_for_entry(&file_node_id, &group.file, related_count),
    );
    let mut parts = project_rule_matches(node_map, edge_map, group, &file_node_id, related_count);
    parts.extend(project_rule_timelines(
        node_map,
        edge_map,
        group,
        &file_node_id,
        related_count,
    ));
    parts.node_ids.insert(0, file_node_id);
    parts.dedup();
    push_rule_results(group, parts, clusters, leads);
}

fn project_rule_matches(
    node_map: &mut NodeMap,
    edge_map: &mut EdgeMap,
    group: &CorrelationRuleGroup,
    file_node_id: &str,
    related_count: u32,
) -> ProjectionParts {
    let mut parts = ProjectionParts {
        jumps: vec![file_jump(group)],
        ..ProjectionParts::default()
    };
    for rule in &group.matches {
        let node = build_artifact_node(&rule.artifact, related_count);
        let node_id = node.id.clone();
        let edge_id = format!(
            "edge:rule:{}:{}:{}",
            rule.artifact.id,
            group.file.id.0,
            edge_kind_token(&rule.kind)
        );
        insert_node(node_map, node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: rule.kind.clone(),
                from_node_id: node_id.clone(),
                to_node_id: file_node_id.to_string(),
                summary: rule.summary.clone(),
                confidence: rule.confidence.clone(),
            });
        parts.node_ids.push(node_id.clone());
        parts.edge_ids.push(edge_id);
        parts.supporting_node_ids.push(node_id);
        parts
            .provenance
            .push(build_artifact_provenance(&rule.artifact));
        if parts.jumps.len() == 1 {
            parts.jumps.push(artifact_jump(&rule.artifact.id));
        }
    }
    parts
}

fn project_rule_timelines(
    node_map: &mut NodeMap,
    edge_map: &mut EdgeMap,
    group: &CorrelationRuleGroup,
    file_node_id: &str,
    related_count: u32,
) -> ProjectionParts {
    let confidence = rule_group_confidence(
        &group.matches,
        group.timelines.len() as u32,
        !group.timeline_signals.is_empty(),
    );
    let mut parts = ProjectionParts::default();
    for timeline in &group.timelines {
        let node = build_timeline_node(timeline, related_count);
        let node_id = node.id.clone();
        let edge_id = format!("edge:rule-timeline:{}:{}", timeline.id, group.file.id.0);
        insert_node(node_map, node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::TemporalContext,
                from_node_id: node_id.clone(),
                to_node_id: file_node_id.to_string(),
                summary: "关联文件时间线事件提供上下文".to_string(),
                confidence: confidence.clone(),
            });
        parts.node_ids.push(node_id.clone());
        parts.edge_ids.push(edge_id);
        parts.supporting_node_ids.push(node_id);
        parts.provenance.push(build_timeline_provenance(timeline));
    }
    parts
}

fn push_rule_results(
    group: &CorrelationRuleGroup,
    parts: ProjectionParts,
    clusters: &mut Vec<CorrelationClusterDto>,
    leads: &mut Vec<CorrelationLeadDto>,
) {
    let confidence = rule_group_confidence(
        &group.matches,
        group.timelines.len() as u32,
        !group.timeline_signals.is_empty(),
    );
    let title = group.file.name.clone();
    let summary = rule_group_summary(
        &group.matches,
        group.timelines.len() as u32,
        group.timeline_signals.len() as u32,
    );
    let families = derive_rule_group_families(group);
    clusters.push(CorrelationClusterDto {
        id: format!("cluster:rules:{}", group.file.id.0),
        title: format!("{title} 规则命中"),
        summary: summary.clone(),
        confidence: confidence.clone(),
        families: families.clone(),
        primary_file_id: group.file.id.0.clone(),
        artifact_count: group.matches.len() as u32,
        timeline_count: group.timelines.len() as u32,
        node_ids: parts.node_ids,
        edge_ids: parts.edge_ids,
        provenance: parts.provenance.clone(),
    });
    leads.push(CorrelationLeadDto {
        id: format!("lead:rules:{}", group.file.id.0),
        title: format!("{title} 形成规则型关联线索"),
        summary,
        confidence,
        families,
        primary_file_id: group.file.id.0.clone(),
        supporting_node_ids: parts.supporting_node_ids,
        match_signals: rule_group_match_signals(
            &group.matches,
            group.timelines.len() as u32,
            &group.timeline_signals,
        ),
        jumps: parts.jumps,
        provenance: parts.provenance,
        caveats: rule_group_caveats(
            &group.matches,
            group.timelines.len() as u32,
            !group.timeline_signals.is_empty(),
        ),
    });
}

fn file_jump(group: &CorrelationRuleGroup) -> CorrelationJumpTargetDto {
    CorrelationJumpTargetDto {
        route: "/files".to_string(),
        target_id: group.file.id.0.clone(),
        label: "查看文件".to_string(),
    }
}

fn artifact_jump(artifact_id: &str) -> CorrelationJumpTargetDto {
    CorrelationJumpTargetDto {
        route: "/artifacts".to_string(),
        target_id: artifact_id.to_string(),
        label: "查看痕迹".to_string(),
    }
}
