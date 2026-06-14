use chrono::Utc;
use domain::{EdgeType, EntryType, FileEntry, FileEntryId, GraphEdge};
use persistence_sqlite::repositories::{file_repo::FileRepo, graph_repo::GraphRepo};
use rusqlite::Connection;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use transport::dto::{
    ArtifactRowDto, CorrelationClusterDto, CorrelationConfidenceDto, CorrelationCoverageStatusDto,
    CorrelationEdgeDto, CorrelationEdgeKindDto, CorrelationFamilyCoverageDto,
    CorrelationJumpTargetDto, CorrelationLeadDto, CorrelationNodeDto, CorrelationNodeKindDto,
    CorrelationProvenanceDto, CorrelationSnapshotDto, TimelineEventDto,
    VerificationGuaranteeLevelDto,
};

const MAX_CORRELATION_ARTIFACTS: usize = 250;
const MAX_CORRELATION_TIMELINE_ROWS: u32 = 250;
const RULE_TIMELINE_CONTEXT_LIMIT: usize = 3;
const RULE_TIMELINE_PROXIMITY_WINDOW_SECS: i64 = 24 * 60 * 60;
const CORRELATION_RULE_FAMILIES: [(&str, &str); 8] = [
    ("LNK", "LNK"),
    ("Prefetch", "Prefetch"),
    ("Registry", "Registry"),
    ("RecycleBin", "Recycle Bin"),
    ("BrowserDownload", "Browser Download"),
    ("BrowserHistory", "Browser History"),
    ("EmailMessage", "Email"),
    ("JumpList", "JumpList"),
];

#[derive(Debug, Default)]
struct CorrelationSourceGroup {
    source_object_id: String,
    file: Option<FileEntry>,
    artifacts: Vec<ArtifactRowDto>,
    timelines: Vec<TimelineEventDto>,
}

#[derive(Debug, Clone)]
struct CorrelationRuleMatch {
    artifact: ArtifactRowDto,
    file: FileEntry,
    kind: CorrelationEdgeKindDto,
    confidence: CorrelationConfidenceDto,
    summary: String,
    caveat: String,
}

#[derive(Debug, Clone)]
struct CorrelationRuleGroup {
    file: FileEntry,
    matches: Vec<CorrelationRuleMatch>,
    timelines: Vec<TimelineEventDto>,
    timeline_signals: Vec<String>,
}

pub fn get_correlation_snapshot(conn: &Connection) -> Result<CorrelationSnapshotDto, String> {
    let artifacts = crate::artifact_service::get_artifact_rows_from_db(conn, None)?
        .into_iter()
        .take(MAX_CORRELATION_ARTIFACTS)
        .collect::<Vec<_>>();
    let timelines =
        crate::timeline_service::query_timeline(conn, 0, MAX_CORRELATION_TIMELINE_ROWS)?
            .items
            .into_iter()
            .filter(|row| !row.source_object_id.trim().is_empty())
            .collect::<Vec<_>>();

    let groups = build_source_groups(conn, artifacts.clone(), timelines.clone())?;
    let rule_groups = build_rule_groups(conn, &artifacts, &timelines)?;

    let mut node_map = BTreeMap::<String, CorrelationNodeDto>::new();
    let mut edge_map = BTreeMap::<String, CorrelationEdgeDto>::new();
    let mut clusters = Vec::new();
    let mut leads = Vec::new();

    for group in groups {
        append_source_group(
            &group,
            &mut node_map,
            &mut edge_map,
            &mut clusters,
            &mut leads,
        );
    }

    for group in rule_groups {
        append_rule_group(
            &group,
            &mut node_map,
            &mut edge_map,
            &mut clusters,
            &mut leads,
        );
    }

    let mut nodes = node_map.into_values().collect::<Vec<_>>();
    nodes.sort_by_key(|node| (node.kind.clone(), node.title.clone(), node.id.clone()));

    let mut edges = edge_map.into_values().collect::<Vec<_>>();
    edges.sort_by_key(|edge| (Reverse(confidence_rank(&edge.confidence)), edge.id.clone()));

    leads.sort_by_key(|lead| {
        (
            Reverse(confidence_rank(&lead.confidence)),
            Reverse(lead.supporting_node_ids.len()),
            lead.title.clone(),
        )
    });
    clusters.sort_by_key(|cluster| {
        (
            Reverse(confidence_rank(&cluster.confidence)),
            Reverse(cluster.node_ids.len()),
            cluster.title.clone(),
        )
    });
    let family_coverage = build_family_coverage(&leads, &clusters);

    // Persist CorrelatesWith edges into the investigative graph
    let _ = persist_correlation_edges(conn, &leads);

    Ok(CorrelationSnapshotDto {
        generated_at: Utc::now().to_rfc3339(),
        node_count: nodes.len() as u32,
        edge_count: edges.len() as u32,
        cluster_count: clusters.len() as u32,
        lead_count: leads.len() as u32,
        family_coverage,
        nodes,
        edges,
        clusters,
        leads,
    })
}

fn build_family_coverage(
    leads: &[CorrelationLeadDto],
    clusters: &[CorrelationClusterDto],
) -> Vec<CorrelationFamilyCoverageDto> {
    CORRELATION_RULE_FAMILIES
        .iter()
        .map(|(family, display_name)| {
            let family_token = family.to_ascii_lowercase();
            let related_leads = leads
                .iter()
                .filter(|lead| {
                    has_family(&lead.families, family)
                        || lead.provenance.iter().any(|item| {
                            item.source_label.eq_ignore_ascii_case(family)
                                || item.source_kind.eq_ignore_ascii_case(family)
                                || item
                                    .producer
                                    .as_deref()
                                    .map(|producer| {
                                        producer.to_ascii_lowercase().contains(&family_token)
                                    })
                                    .unwrap_or(false)
                        })
                        || lead
                            .match_signals
                            .iter()
                            .any(|signal| signal.to_ascii_lowercase().contains(&family_token))
                })
                .collect::<Vec<_>>();

            let cluster_count = clusters
                .iter()
                .filter(|cluster| {
                    has_family(&cluster.families, family)
                        || cluster.provenance.iter().any(|item| {
                            item.source_label.eq_ignore_ascii_case(family)
                                || item.source_kind.eq_ignore_ascii_case(family)
                                || item
                                    .producer
                                    .as_deref()
                                    .map(|producer| {
                                        producer.to_ascii_lowercase().contains(&family_token)
                                    })
                                    .unwrap_or(false)
                        })
                        || cluster.summary.to_ascii_lowercase().contains(&family_token)
                })
                .count() as u32;

            let lead_count = related_leads.len() as u32;
            let high_confidence_lead_count = related_leads
                .iter()
                .filter(|lead| {
                    matches!(
                        lead.confidence,
                        CorrelationConfidenceDto::Direct | CorrelationConfidenceDto::Strong
                    )
                })
                .count() as u32;
            let review_lead_count = related_leads
                .iter()
                .filter(|lead| {
                    !lead.caveats.is_empty()
                        || matches!(
                            lead.confidence,
                            CorrelationConfidenceDto::Weak | CorrelationConfidenceDto::Heuristic
                        )
                })
                .count() as u32;
            let mut sample_signals = related_leads
                .iter()
                .flat_map(|lead| lead.match_signals.iter().cloned())
                .filter(|signal| signal.to_ascii_lowercase().contains(&family_token))
                .take(3)
                .collect::<Vec<_>>();
            if sample_signals.is_empty() {
                sample_signals = related_leads
                    .iter()
                    .flat_map(|lead| lead.match_signals.iter().cloned())
                    .take(3)
                    .collect::<Vec<_>>();
            }

            let status = if lead_count == 0 {
                CorrelationCoverageStatusDto::Missing
            } else if high_confidence_lead_count > 0 {
                CorrelationCoverageStatusDto::Covered
            } else {
                CorrelationCoverageStatusDto::Review
            };

            CorrelationFamilyCoverageDto {
                family: (*family).to_string(),
                display_name: (*display_name).to_string(),
                status,
                lead_count,
                high_confidence_lead_count,
                review_lead_count,
                cluster_count,
                sample_signals,
            }
        })
        .collect()
}

fn has_family(families: &[String], family: &str) -> bool {
    families
        .iter()
        .any(|item| item.eq_ignore_ascii_case(family))
}

fn derive_source_group_families(group: &CorrelationSourceGroup) -> Vec<String> {
    let mut families = group
        .artifacts
        .iter()
        .filter_map(|artifact| artifact_family(&artifact.artifact_type))
        .collect::<Vec<_>>();
    dedup_vec(&mut families);
    families
}

fn derive_rule_group_families(group: &CorrelationRuleGroup) -> Vec<String> {
    let mut families = group
        .matches
        .iter()
        .filter_map(|item| artifact_family(&item.artifact.artifact_type))
        .collect::<Vec<_>>();
    dedup_vec(&mut families);
    families
}

fn artifact_family(artifact_type: &str) -> Option<String> {
    match artifact_type {
        "RegistryValue" => Some("Registry".to_string()),
        value => CORRELATION_RULE_FAMILIES
            .iter()
            .find(|(family, _)| family.eq_ignore_ascii_case(value))
            .map(|(family, _)| (*family).to_string()),
    }
}

fn append_source_group(
    group: &CorrelationSourceGroup,
    node_map: &mut BTreeMap<String, CorrelationNodeDto>,
    edge_map: &mut BTreeMap<String, CorrelationEdgeDto>,
    clusters: &mut Vec<CorrelationClusterDto>,
    leads: &mut Vec<CorrelationLeadDto>,
) {
    let file_node_id = format!("file:{}", group.source_object_id);
    let artifact_count = group.artifacts.len() as u32;
    let timeline_count = group.timelines.len() as u32;
    let related_count = artifact_count + timeline_count;
    let confidence = group_confidence(artifact_count, timeline_count);
    let file_node = build_file_node(&file_node_id, group, related_count);
    let mut node_ids = vec![file_node.id.clone()];
    let mut edge_ids = Vec::new();
    let mut supporting_node_ids = Vec::new();
    let mut provenance = Vec::new();

    insert_node(node_map, file_node);

    for artifact in &group.artifacts {
        let artifact_node = build_artifact_node(artifact, related_count);
        let artifact_node_id = artifact_node.id.clone();
        let edge_id = format!("edge:{}:{}", artifact.id, group.source_object_id);

        insert_node(node_map, artifact_node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::SourceReference,
                from_node_id: artifact_node_id.clone(),
                to_node_id: file_node_id.clone(),
                summary: format!("{} 引用同一 source object", artifact.artifact_type),
                confidence: CorrelationConfidenceDto::Direct,
            });

        node_ids.push(artifact_node_id.clone());
        edge_ids.push(edge_id);
        supporting_node_ids.push(artifact_node_id);
        provenance.push(build_artifact_provenance(artifact));
    }

    for timeline in &group.timelines {
        let timeline_node = build_timeline_node(timeline, related_count);
        let timeline_node_id = timeline_node.id.clone();
        let edge_id = format!("edge:{}:{}", timeline.id, group.source_object_id);

        insert_node(node_map, timeline_node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::TemporalContext,
                from_node_id: timeline_node_id.clone(),
                to_node_id: file_node_id.clone(),
                summary: format!("{} 时间线事件命中同一 source object", timeline.event_type),
                confidence: if artifact_count > 0 {
                    CorrelationConfidenceDto::Direct
                } else {
                    CorrelationConfidenceDto::Strong
                },
            });

        node_ids.push(timeline_node_id.clone());
        edge_ids.push(edge_id);
        supporting_node_ids.push(timeline_node_id);
        provenance.push(build_timeline_provenance(timeline));
    }

    if let (Some(artifact), Some(timeline)) = (group.artifacts.first(), group.timelines.first()) {
        let edge_id = format!("edge:shared:{}:{}", artifact.id, timeline.id);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::SharedSourceObject,
                from_node_id: format!("artifact:{}", artifact.id),
                to_node_id: format!("timeline:{}", timeline.id),
                summary: "Artifact 与时间线共享同一 source object".to_string(),
                confidence: CorrelationConfidenceDto::Direct,
            });
        edge_ids.push(edge_id);
    }

    let title = group_title(group);
    let summary = group_summary(group);
    let caveats = group_caveats(group, artifact_count, timeline_count);
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
        node_ids,
        edge_ids,
        provenance: provenance.clone(),
    });

    leads.push(CorrelationLeadDto {
        id: format!("lead:{}", group.source_object_id),
        title: format!("{title} 形成关联线索"),
        summary,
        confidence,
        families,
        primary_file_id: group.source_object_id.clone(),
        supporting_node_ids,
        match_signals: source_group_match_signals(group),
        jumps: build_lead_jumps(group),
        provenance,
        caveats,
    });
}

fn append_rule_group(
    group: &CorrelationRuleGroup,
    node_map: &mut BTreeMap<String, CorrelationNodeDto>,
    edge_map: &mut BTreeMap<String, CorrelationEdgeDto>,
    clusters: &mut Vec<CorrelationClusterDto>,
    leads: &mut Vec<CorrelationLeadDto>,
) {
    let file_node_id = format!("file:{}", group.file.id.0);
    let related_count = (group.matches.len() + group.timelines.len()) as u32;
    insert_node(
        node_map,
        build_file_node_for_entry(&file_node_id, &group.file, related_count),
    );

    let mut node_ids = vec![file_node_id.clone()];
    let mut edge_ids = Vec::new();
    let mut supporting_node_ids = Vec::new();
    let mut provenance = Vec::new();
    let mut jumps = vec![CorrelationJumpTargetDto {
        route: "/files".to_string(),
        target_id: group.file.id.0.clone(),
        label: "查看文件".to_string(),
    }];

    for rule in &group.matches {
        let artifact_node = build_artifact_node(&rule.artifact, related_count);
        let artifact_node_id = artifact_node.id.clone();
        let edge_id = format!(
            "edge:rule:{}:{}:{}",
            rule.artifact.id,
            group.file.id.0,
            edge_kind_token(&rule.kind)
        );

        insert_node(node_map, artifact_node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: rule.kind.clone(),
                from_node_id: artifact_node_id.clone(),
                to_node_id: file_node_id.clone(),
                summary: rule.summary.clone(),
                confidence: rule.confidence.clone(),
            });

        node_ids.push(artifact_node_id.clone());
        edge_ids.push(edge_id);
        supporting_node_ids.push(artifact_node_id);
        provenance.push(build_artifact_provenance(&rule.artifact));

        if jumps.len() == 1 {
            jumps.push(CorrelationJumpTargetDto {
                route: "/artifacts".to_string(),
                target_id: rule.artifact.id.clone(),
                label: "查看痕迹".to_string(),
            });
        }
    }

    let rule_group_confidence = rule_group_confidence(
        &group.matches,
        group.timelines.len() as u32,
        !group.timeline_signals.is_empty(),
    );
    for timeline in &group.timelines {
        let timeline_node = build_timeline_node(timeline, related_count);
        let timeline_node_id = timeline_node.id.clone();
        let edge_id = format!("edge:rule-timeline:{}:{}", timeline.id, group.file.id.0);

        insert_node(node_map, timeline_node);
        edge_map
            .entry(edge_id.clone())
            .or_insert(CorrelationEdgeDto {
                id: edge_id.clone(),
                kind: CorrelationEdgeKindDto::TemporalContext,
                from_node_id: timeline_node_id.clone(),
                to_node_id: file_node_id.clone(),
                summary: "关联文件时间线事件提供上下文".to_string(),
                confidence: rule_group_confidence.clone(),
            });

        node_ids.push(timeline_node_id.clone());
        edge_ids.push(edge_id);
        supporting_node_ids.push(timeline_node_id);
        provenance.push(build_timeline_provenance(timeline));
    }

    dedup_vec(&mut node_ids);
    dedup_vec(&mut edge_ids);
    dedup_vec(&mut supporting_node_ids);
    dedup_vec(&mut jumps);
    dedup_vec(&mut provenance);

    let confidence = rule_group_confidence;
    let title = group.file.name.clone();
    let summary = rule_group_summary(
        &group.matches,
        group.timelines.len() as u32,
        group.timeline_signals.len() as u32,
    );
    let caveats = rule_group_caveats(
        &group.matches,
        group.timelines.len() as u32,
        !group.timeline_signals.is_empty(),
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
        node_ids,
        edge_ids,
        provenance: provenance.clone(),
    });

    leads.push(CorrelationLeadDto {
        id: format!("lead:rules:{}", group.file.id.0),
        title: format!("{title} 形成规则型关联线索"),
        summary,
        confidence,
        families,
        primary_file_id: group.file.id.0.clone(),
        supporting_node_ids,
        match_signals: rule_group_match_signals(
            &group.matches,
            group.timelines.len() as u32,
            &group.timeline_signals,
        ),
        jumps,
        provenance,
        caveats,
    });
}

fn build_source_groups(
    conn: &Connection,
    artifacts: Vec<ArtifactRowDto>,
    timelines: Vec<TimelineEventDto>,
) -> Result<Vec<CorrelationSourceGroup>, String> {
    let mut groups = BTreeMap::<String, CorrelationSourceGroup>::new();

    for artifact in artifacts {
        let Some(source_object_id) = artifact.source_object_id.clone() else {
            continue;
        };
        let group =
            groups
                .entry(source_object_id.clone())
                .or_insert_with(|| CorrelationSourceGroup {
                    source_object_id,
                    ..CorrelationSourceGroup::default()
                });
        group.artifacts.push(artifact);
    }

    for timeline in timelines {
        let source_object_id = timeline.source_object_id.clone();
        let group =
            groups
                .entry(source_object_id.clone())
                .or_insert_with(|| CorrelationSourceGroup {
                    source_object_id,
                    ..CorrelationSourceGroup::default()
                });
        group.timelines.push(timeline);
    }

    let repo = FileRepo::new(conn);
    let mut items = groups.into_values().collect::<Vec<_>>();
    for group in &mut items {
        group.file = repo
            .find_by_id(&FileEntryId(group.source_object_id.clone()))
            .map_err(|e| e.to_string())?;
    }
    items.sort_by_key(|group| {
        (
            Reverse(group.artifacts.len() + group.timelines.len()),
            Reverse(group.artifacts.len()),
            group_title(group),
        )
    });
    Ok(items)
}

fn build_rule_groups(
    conn: &Connection,
    artifacts: &[ArtifactRowDto],
    timelines: &[TimelineEventDto],
) -> Result<Vec<CorrelationRuleGroup>, String> {
    let files = crate::analysis_service::collect_file_entries(conn)?;
    let timeline_map = timelines.iter().fold(
        BTreeMap::<String, Vec<TimelineEventDto>>::new(),
        |mut acc, item| {
            acc.entry(item.source_object_id.clone())
                .or_default()
                .push(item.clone());
            acc
        },
    );
    let mut groups = BTreeMap::<String, CorrelationRuleGroup>::new();

    for artifact in artifacts {
        for rule_match in build_artifact_rule_matches(&files, artifact) {
            let group = groups
                .entry(rule_match.file.id.0.clone())
                .or_insert_with(|| CorrelationRuleGroup {
                    file: rule_match.file.clone(),
                    matches: Vec::new(),
                    timelines: timeline_map
                        .get(&rule_match.file.id.0)
                        .cloned()
                        .unwrap_or_default(),
                    timeline_signals: Vec::new(),
                });

            let exists = group.matches.iter().any(|existing| {
                existing.artifact.id == rule_match.artifact.id
                    && existing.kind == rule_match.kind
                    && existing.file.id == rule_match.file.id
            });
            if !exists {
                group.matches.push(rule_match);
            }
        }
    }

    let mut items = groups.into_values().collect::<Vec<_>>();
    for group in &mut items {
        group.timeline_signals = derive_rule_timeline_signals(group, timelines);
        group.matches.sort_by_key(|rule| {
            (
                Reverse(confidence_rank(&rule.confidence)),
                rule.artifact.artifact_type.clone(),
                rule.artifact.id.clone(),
            )
        });
    }
    items.sort_by_key(|group| {
        (
            Reverse(confidence_rank(&rule_group_confidence(
                &group.matches,
                group.timelines.len() as u32,
                !group.timeline_signals.is_empty(),
            ))),
            Reverse(group.matches.len()),
            group.file.path.clone(),
        )
    });
    Ok(items)
}

fn derive_rule_timeline_signals(
    group: &CorrelationRuleGroup,
    all_timelines: &[TimelineEventDto],
) -> Vec<String> {
    let mut signals = Vec::new();

    if !group.timelines.is_empty() {
        signals.push(format!(
            "关联文件自身已有 {} 条 timeline 事件",
            group.timelines.len()
        ));
    }

    let artifact_times = group
        .matches
        .iter()
        .flat_map(rule_match_timestamps)
        .collect::<Vec<_>>();

    if artifact_times.is_empty() {
        return signals;
    }

    let target_path_keys = group
        .matches
        .iter()
        .flat_map(rule_match_paths)
        .map(|value| path_suffix_key(&value))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    let text_needles = group
        .matches
        .iter()
        .flat_map(rule_match_text_needles)
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();

    if target_path_keys.is_empty() && text_needles.is_empty() {
        return signals;
    }

    let mut related = all_timelines
        .iter()
        .filter(|timeline| timeline.source_object_id != group.file.id.0)
        .filter_map(|timeline| {
            let timeline_ts = parse_rfc3339_utc(&timeline.ts)?;
            let within_window = artifact_times.iter().any(|artifact_ts| {
                (timeline_ts.timestamp() - artifact_ts.timestamp()).abs()
                    <= RULE_TIMELINE_PROXIMITY_WINDOW_SECS
            });
            if !within_window {
                return None;
            }

            let timeline_paths = timeline_path_candidates(timeline);
            let path_hit = timeline_paths.into_iter().any(|candidate| {
                let suffix = path_suffix_key(&candidate);
                !suffix.is_empty() && target_path_keys.contains(&suffix)
            });
            let text_hit = timeline_text_candidates(timeline)
                .into_iter()
                .any(|candidate| {
                    let normalized = candidate.to_ascii_lowercase();
                    text_needles
                        .iter()
                        .any(|needle| !needle.is_empty() && normalized.contains(needle))
                });
            if !path_hit && !text_hit {
                return None;
            }

            Some(format!("{} @ {}", timeline.event_type, timeline.ts))
        })
        .collect::<Vec<_>>();

    related.sort();
    related.truncate(RULE_TIMELINE_CONTEXT_LIMIT);
    for item in related {
        signals.push(format!("邻近时间线命中 {item}"));
    }

    signals
}

fn build_artifact_rule_matches(
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

fn build_single_path_rule(
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

fn build_registry_rules(
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

fn build_browser_history_rules(
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

fn build_name_rules(
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

fn build_email_subject_rules(
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

fn dedup_rule_matches(matches: &mut Vec<CorrelationRuleMatch>) {
    let mut seen = BTreeSet::new();
    matches.retain(|item| {
        seen.insert((
            item.artifact.id.clone(),
            item.file.id.0.clone(),
            item.kind.clone(),
        ))
    });
}

fn build_file_node(
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

fn build_file_node_for_entry(
    file_node_id: &str,
    file: &FileEntry,
    related_count: u32,
) -> CorrelationNodeDto {
    let title = if file.entry_type == EntryType::Directory {
        format!("{}/", file.name)
    } else {
        file.name.clone()
    };
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

    CorrelationNodeDto {
        id: file_node_id.to_string(),
        kind: CorrelationNodeKindDto::File,
        title,
        subtitle: Some(file.path.clone()),
        source_object_id: Some(file.id.0.clone()),
        related_count,
        badges,
        jumps: vec![CorrelationJumpTargetDto {
            route: "/files".to_string(),
            target_id: file.id.0.clone(),
            label: "打开文件浏览".to_string(),
        }],
    }
}

fn build_artifact_node(artifact: &ArtifactRowDto, related_count: u32) -> CorrelationNodeDto {
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

fn build_timeline_node(timeline: &TimelineEventDto, related_count: u32) -> CorrelationNodeDto {
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

fn build_artifact_provenance(artifact: &ArtifactRowDto) -> CorrelationProvenanceDto {
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

fn build_timeline_provenance(timeline: &TimelineEventDto) -> CorrelationProvenanceDto {
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

fn build_lead_jumps(group: &CorrelationSourceGroup) -> Vec<CorrelationJumpTargetDto> {
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

fn group_title(group: &CorrelationSourceGroup) -> String {
    group
        .file
        .as_ref()
        .map(|file| file.name.clone())
        .unwrap_or_else(|| group.source_object_id.clone())
}

fn group_summary(group: &CorrelationSourceGroup) -> String {
    let artifact_count = group.artifacts.len();
    let timeline_count = group.timelines.len();
    match (artifact_count, timeline_count) {
        (0, timeline_count) => format!("同一 source object 命中 {timeline_count} 条时间线事件。"),
        (artifact_count, 0) => format!("同一 source object 命中 {artifact_count} 条痕迹记录。"),
        (artifact_count, timeline_count) => {
            format!("同一 source object 聚合 {artifact_count} 条痕迹记录与 {timeline_count} 条时间线事件。")
        }
    }
}

fn group_caveats(
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

fn source_group_match_signals(group: &CorrelationSourceGroup) -> Vec<String> {
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

fn rule_group_match_signals(
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

fn rule_group_summary(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    proximity_timeline_count: u32,
) -> String {
    let mut path_matches = 0usize;
    let mut name_matches = 0usize;
    let mut recovered_matches = 0usize;
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

    let families = artifact_types.into_iter().collect::<Vec<_>>().join(" / ");
    format!(
        "{families} 规则命中 {} 条记录（路径 {}，名称 {}，原路径恢复 {}，自身时间线 {}，邻近时间线 {}）。",
        matches.len(),
        path_matches,
        name_matches,
        recovered_matches,
        timeline_count,
        proximity_timeline_count
    )
}

fn rule_group_caveats(
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

fn artifact_guarantee_level(artifact_type: &str) -> VerificationGuaranteeLevelDto {
    match artifact_type {
        "Prefetch" | "LNK" | "Registry" | "RegistryValue" | "RecycleBin" => {
            VerificationGuaranteeLevelDto::BestEffort
        }
        "BrowserDownload" | "BrowserHistory" | "EmailMessage" => {
            VerificationGuaranteeLevelDto::Experimental
        }
        _ => VerificationGuaranteeLevelDto::Experimental,
    }
}

fn timeline_guarantee_level(parser_id: Option<&str>) -> VerificationGuaranteeLevelDto {
    match parser_id {
        Some(parser_id) if parser_id.starts_with("timeline.") => {
            VerificationGuaranteeLevelDto::BestEffort
        }
        Some(parser_id) if parser_id.starts_with("evtx.") => {
            VerificationGuaranteeLevelDto::BestEffort
        }
        _ => VerificationGuaranteeLevelDto::Experimental,
    }
}

fn group_confidence(artifact_count: u32, timeline_count: u32) -> CorrelationConfidenceDto {
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

fn rule_group_confidence(
    matches: &[CorrelationRuleMatch],
    timeline_count: u32,
    has_proximity_timeline: bool,
) -> CorrelationConfidenceDto {
    if matches
        .iter()
        .any(|item| item.kind == CorrelationEdgeKindDto::RecoveredOriginalPath)
    {
        return CorrelationConfidenceDto::Direct;
    }
    if matches
        .iter()
        .any(|item| item.kind == CorrelationEdgeKindDto::PathMatch)
    {
        return CorrelationConfidenceDto::Direct;
    }
    if (timeline_count > 0 || has_proximity_timeline) && !matches.is_empty() {
        return CorrelationConfidenceDto::Strong;
    }
    if matches.len() >= 2
        || matches
            .iter()
            .any(|item| item.confidence == CorrelationConfidenceDto::Strong)
    {
        return CorrelationConfidenceDto::Strong;
    }
    if !matches.is_empty() {
        return CorrelationConfidenceDto::Weak;
    }
    CorrelationConfidenceDto::Heuristic
}

fn confidence_rank(confidence: &CorrelationConfidenceDto) -> u8 {
    match confidence {
        CorrelationConfidenceDto::Direct => 4,
        CorrelationConfidenceDto::Strong => 3,
        CorrelationConfidenceDto::Weak => 2,
        CorrelationConfidenceDto::Heuristic => 1,
    }
}

fn edge_kind_token(kind: &CorrelationEdgeKindDto) -> &'static str {
    match kind {
        CorrelationEdgeKindDto::SourceReference => "source",
        CorrelationEdgeKindDto::SharedSourceObject => "shared",
        CorrelationEdgeKindDto::TemporalContext => "temporal",
        CorrelationEdgeKindDto::PathMatch => "path",
        CorrelationEdgeKindDto::NameMatch => "name",
        CorrelationEdgeKindDto::RecoveredOriginalPath => "recovered",
    }
}

fn normalize_path(value: &str) -> String {
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

fn parse_rfc3339_utc(value: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|item| item.with_timezone(&Utc))
}

fn path_suffix_key(value: &str) -> String {
    let normalized = normalize_path(value);
    let bytes = normalized.as_bytes();
    if normalized.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' {
        normalized[3..].to_string()
    } else {
        normalized.trim_start_matches('/').to_string()
    }
}

fn basename(value: &str) -> String {
    normalize_path(value)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn looks_like_path(value: &str) -> bool {
    let candidate = value.trim();
    candidate.contains(":\\")
        || candidate.contains(":/")
        || candidate.starts_with("\\\\")
        || candidate.starts_with("//")
}

fn extract_path_candidates(value: &str) -> Vec<String> {
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

fn extract_file_name_candidates(value: &str) -> Vec<String> {
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

fn extract_quoted_segments(value: &str) -> Vec<String> {
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

fn rule_match_timestamps(rule: &CorrelationRuleMatch) -> Vec<chrono::DateTime<Utc>> {
    match rule.artifact.artifact_type.as_str() {
        "BrowserHistory" => first_string_attr(&rule.artifact.attrs, &["visitTime"])
            .and_then(|value| parse_rfc3339_utc(&value))
            .into_iter()
            .collect(),
        "BrowserDownload" => first_string_attr(&rule.artifact.attrs, &["startTime"])
            .and_then(|value| parse_rfc3339_utc(&value))
            .into_iter()
            .collect(),
        "EmailMessage" => first_string_attr(&rule.artifact.attrs, &["sentAt"])
            .and_then(|value| parse_rfc3339_utc(&value))
            .into_iter()
            .collect(),
        "RecycleBin" => Vec::new(),
        _ => Vec::new(),
    }
}

fn rule_match_paths(rule: &CorrelationRuleMatch) -> Vec<String> {
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

fn rule_match_text_needles(rule: &CorrelationRuleMatch) -> Vec<String> {
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

fn timeline_path_candidates(timeline: &TimelineEventDto) -> Vec<String> {
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

fn timeline_text_candidates(timeline: &TimelineEventDto) -> Vec<String> {
    let mut candidates = vec![timeline.title.clone(), timeline.description.clone()];
    if let Some(value) = first_string_attr(&timeline.attrs, &["url", "title"]) {
        candidates.push(value);
    }
    candidates
}

fn first_string_attr(attrs: &BTreeMap<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| attrs.get(*key))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn string_array_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    attrs
        .get(key)
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(|item| item.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn find_best_file_by_path<'a>(
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
        .filter(|file| deleted_preference_score(file, prefer_deleted) < 2)
        .filter(|file| normalize_path(&file.path) == normalized)
        .min_by_key(|file| {
            (
                deleted_preference_score(file, prefer_deleted),
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
        .filter(|file| deleted_preference_score(file, prefer_deleted) < 2)
        .filter(|file| path_suffix_key(&file.path).ends_with(&suffix))
        .min_by_key(|file| {
            (
                deleted_preference_score(file, prefer_deleted),
                file.path.len(),
            )
        })
}

fn find_best_file_by_name<'a>(
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
        .filter(|file| deleted_preference_score(file, prefer_deleted) < 2)
        .min_by_key(|file| {
            (
                deleted_preference_score(file, prefer_deleted),
                file.path.len(),
            )
        })
}

fn deleted_preference_score(file: &FileEntry, prefer_deleted: Option<bool>) -> u8 {
    match prefer_deleted {
        Some(expected) if file.deleted == expected => 0,
        Some(_) => 1,
        None => 0,
    }
}

fn insert_node(map: &mut BTreeMap<String, CorrelationNodeDto>, node: CorrelationNodeDto) {
    match map.get_mut(&node.id) {
        Some(existing) => {
            existing.related_count = existing.related_count.max(node.related_count);
            existing.badges.extend(node.badges);
            existing.jumps.extend(node.jumps);
            dedup_vec(&mut existing.badges);
            dedup_vec(&mut existing.jumps);
            if existing.subtitle.is_none() {
                existing.subtitle = node.subtitle;
            }
            if existing.source_object_id.is_none() {
                existing.source_object_id = node.source_object_id;
            }
        }
        None => {
            map.insert(node.id.clone(), node);
        }
    }
}

fn dedup_vec<T>(values: &mut Vec<T>)
where
    T: Clone + PartialEq,
{
    let mut deduped = Vec::new();
    for item in values.iter().cloned() {
        if !deduped.contains(&item) {
            deduped.push(item);
        }
    }
    *values = deduped;
}

/// Persist CorrelatesWith graph edges from correlation leads into the investigative graph.
///
/// For each lead, creates CorrelatesWith edges linking each supporting artifact node
/// to the lead's primary file node. The edge carries confidence and rule provenance
/// (serialised match signals).
fn persist_correlation_edges(
    conn: &Connection,
    leads: &[CorrelationLeadDto],
) -> Result<(), String> {
    if leads.is_empty() {
        return Ok(());
    }

    // Resolve case_id from the artifacts table
    let case_id: String = match conn.query_row(
        "SELECT DISTINCT case_id FROM artifacts LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(id) => id,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(e) => return Err(format!("resolve case_id for correlation edges: {e}")),
    };

    let graph_repo = GraphRepo::new(conn);
    let now = Utc::now().to_rfc3339();
    let mut edges: Vec<GraphEdge> = Vec::new();

    for lead in leads {
        let confidence = map_correlation_confidence(&lead.confidence);
        let provenance = build_correlation_provenance(lead);

        for node_id in &lead.supporting_node_ids {
            // Only process artifact nodes (prefixed "artifact:")
            let artifact_id = match node_id.strip_prefix("artifact:") {
                Some(raw) => raw,
                None => continue,
            };

            edges.push(GraphEdge {
                id: format!("correlates_with:{artifact_id}:{}", lead.primary_file_id),
                case_id: case_id.clone(),
                source_id: artifact_id.to_string(),
                target_id: lead.primary_file_id.clone(),
                edge_type: EdgeType::CorrelatesWith,
                confidence: Some(confidence),
                provenance: Some(provenance.clone()),
                created_at: now.clone(),
            });
        }
    }

    if !edges.is_empty() {
        graph_repo
            .insert_edges_batch(&edges)
            .map_err(|e| format!("correlation graph edge insert: {e}"))?;
    }

    Ok(())
}

/// Map CorrelationConfidenceDto to a numeric confidence value in [0.0, 1.0].
fn map_correlation_confidence(confidence: &CorrelationConfidenceDto) -> f64 {
    match confidence {
        CorrelationConfidenceDto::Direct => 1.0,
        CorrelationConfidenceDto::Strong => 0.9,
        CorrelationConfidenceDto::Weak => 0.5,
        CorrelationConfidenceDto::Heuristic => 0.3,
    }
}

/// Build a JSON provenance string from a correlation lead's match signals.
fn build_correlation_provenance(lead: &CorrelationLeadDto) -> String {
    let signals = if lead.match_signals.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Array(
            lead.match_signals
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        )
    };

    serde_json::json!({
        "kind": "correlation_rule",
        "lead_id": lead.id,
        "match_signals": signals,
        "families": lead.families,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::{
        Artifact, ArtifactId, CaseId, DataSource, DataSourceId, DataSourceKind,
        DataSourceProvenance, EntryType, FileEntry, TimelineEvent, TimelineEventId,
    };
    use persistence_sqlite::repositories::{
        artifact_repo::ArtifactRepo, case_repo::CaseRepo, datasource_repo::DataSourceRepo,
        file_repo::FileRepo, timeline_repo::TimelineRepo,
    };
    use std::collections::BTreeMap;

    fn setup_case_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        CaseRepo::new(&conn)
            .create(&domain::CaseMeta {
                id: CaseId("case-1".to_string()),
                name: "Case".to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-1".to_string()),
                &DataSource {
                    id: DataSourceId("ds-1".to_string()),
                    name: "source".to_string(),
                    kind: DataSourceKind::Raw,
                    source_path: "C:/evidence/mock.raw".into(),
                    imported_at: Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .unwrap();
        conn
    }

    fn insert_file(conn: &Connection, id: &str, path: &str, deleted: bool) {
        FileRepo::new(conn)
            .insert_batch(&[FileEntry {
                id: FileEntryId(id.to_string()),
                parent_id: None,
                data_source_id: DataSourceId("ds-1".to_string()),
                path: path.to_string(),
                name: basename(path),
                entry_type: EntryType::File,
                size: Some(1024),
                ext: Some("exe".to_string()),
                deleted,
                hidden: false,
                system: false,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            }])
            .unwrap();
    }

    fn insert_artifact(
        conn: &Connection,
        id: &str,
        family: &str,
        source_object_id: Option<&str>,
        attrs: BTreeMap<String, Value>,
    ) {
        ArtifactRepo::new(conn)
            .insert_batch(
                &[Artifact {
                    id: ArtifactId(id.to_string()),
                    family: family.to_string(),
                    title: format!("{family} artifact"),
                    summary: "fixture".to_string(),
                    source_object_id: source_object_id.map(|value| FileEntryId(value.to_string())),
                    extractor_id: Some(family.to_ascii_lowercase()),
                    extractor_version: Some("1.0.0".to_string()),
                    confidence: Some(0.91),
                    source_attribution: Some("fixture".to_string()),
                    created_at: Utc::now(),
                    attrs,
                }],
                "case-1",
                "ds-1",
            )
            .unwrap();
    }

    #[test]
    fn correlation_snapshot_groups_artifact_and_timeline_by_source_object() {
        let conn = setup_case_db();
        insert_file(&conn, "file-1", "C:/Windows/System32/cmd.exe", true);
        insert_artifact(
            &conn,
            "artifact-1",
            "Prefetch",
            Some("file-1"),
            BTreeMap::new(),
        );
        TimelineRepo::new(&conn)
            .insert_batch_with_case(
                &[TimelineEvent {
                    id: TimelineEventId("timeline-1".to_string()),
                    source_object_id: "file-1".to_string(),
                    event_type: "FILE_MODIFIED".to_string(),
                    timestamp: Utc::now(),
                    title: "File modified".to_string(),
                    description: "MACB projection".to_string(),
                    parser_id: Some("timeline.macb".to_string()),
                    parser_version: Some("1.0.0".to_string()),
                    confidence: Some(0.82),
                    source_attribution: Some("modified_at".to_string()),
                    attrs: BTreeMap::new(),
                }],
                "case-1",
            )
            .unwrap();

        let snapshot = get_correlation_snapshot(&conn).unwrap();

        assert_eq!(snapshot.cluster_count, 1);
        assert_eq!(snapshot.lead_count, 1);
        assert!(snapshot.node_count >= 3);
        assert!(snapshot.edge_count >= 3);
        assert_eq!(
            snapshot.leads[0].confidence,
            CorrelationConfidenceDto::Direct
        );
        assert_eq!(snapshot.leads[0].primary_file_id, "file-1");
        assert!(snapshot.leads[0].summary.contains("痕迹记录"));
        assert!(snapshot.nodes.iter().any(|node| {
            node.kind == CorrelationNodeKindDto::File
                && node.badges.iter().any(|badge| badge == "deleted")
        }));
        assert!(snapshot.clusters[0]
            .provenance
            .iter()
            .any(|item| item.source_kind == "artifact"
                && item.producer.as_deref() == Some("prefetch")));
    }

    #[test]
    fn correlation_snapshot_matches_lnk_target_path_to_file() {
        let conn = setup_case_db();
        insert_file(&conn, "file-lnk", "C:/Users/Admin/Desktop/cmd.lnk", false);
        insert_file(&conn, "file-cmd", "C:/Windows/System32/cmd.exe", false);

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "target_path".to_string(),
            Value::String("C:/Windows/System32/cmd.exe".to_string()),
        );
        insert_artifact(&conn, "artifact-lnk", "LNK", Some("file-lnk"), attrs);

        let snapshot = get_correlation_snapshot(&conn).unwrap();
        let lead = snapshot
            .leads
            .iter()
            .find(|item| item.id == "lead:rules:file-cmd")
            .unwrap();

        assert_eq!(lead.primary_file_id, "file-cmd");
        assert_eq!(lead.confidence, CorrelationConfidenceDto::Direct);
        assert!(lead.summary.contains("路径"));
        assert!(snapshot.edges.iter().any(|edge| {
            edge.kind == CorrelationEdgeKindDto::PathMatch
                && edge.from_node_id == "artifact:artifact-lnk"
                && edge.to_node_id == "file:file-cmd"
        }));
    }

    #[test]
    fn correlation_snapshot_matches_registry_value_path_to_file() {
        let conn = setup_case_db();
        insert_file(
            &conn,
            "file-reg",
            "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
            false,
        );

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "data".to_string(),
            Value::String(
                "\"C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -nop"
                    .to_string(),
            ),
        );
        insert_artifact(
            &conn,
            "artifact-reg",
            "RegistryValue",
            Some("registry-hive"),
            attrs,
        );

        let snapshot = get_correlation_snapshot(&conn).unwrap();

        assert!(snapshot.edges.iter().any(|edge| {
            edge.kind == CorrelationEdgeKindDto::PathMatch
                && edge.from_node_id == "artifact:artifact-reg"
                && edge.to_node_id == "file:file-reg"
        }));
    }

    #[test]
    fn correlation_snapshot_matches_recycle_bin_original_path_to_deleted_file() {
        let conn = setup_case_db();
        insert_file(
            &conn,
            "file-deleted",
            "C:/Users/Admin/Desktop/secrets.txt",
            true,
        );

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "original_path".to_string(),
            Value::String("C:/Users/Admin/Desktop/secrets.txt".to_string()),
        );
        insert_artifact(&conn, "artifact-rb", "RecycleBin", Some("recycle-i"), attrs);

        let snapshot = get_correlation_snapshot(&conn).unwrap();

        assert!(snapshot.edges.iter().any(|edge| {
            edge.kind == CorrelationEdgeKindDto::RecoveredOriginalPath
                && edge.from_node_id == "artifact:artifact-rb"
                && edge.to_node_id == "file:file-deleted"
        }));
    }

    #[test]
    fn correlation_snapshot_matches_prefetch_executable_name_to_file_name() {
        let conn = setup_case_db();
        insert_file(&conn, "file-cmd", "C:/Windows/System32/cmd.exe", false);

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "executable".to_string(),
            Value::String("CMD.EXE".to_string()),
        );
        insert_artifact(&conn, "artifact-pf", "Prefetch", Some("pf-file"), attrs);

        let snapshot = get_correlation_snapshot(&conn).unwrap();
        let edge = snapshot
            .edges
            .iter()
            .find(|edge| {
                edge.from_node_id == "artifact:artifact-pf"
                    && edge.kind == CorrelationEdgeKindDto::NameMatch
            })
            .unwrap();

        assert_eq!(edge.kind, CorrelationEdgeKindDto::NameMatch);
        assert_eq!(edge.confidence, CorrelationConfidenceDto::Strong);
    }

    #[test]
    fn correlation_snapshot_rule_group_uses_related_timeline_as_context() {
        let conn = setup_case_db();
        insert_file(&conn, "file-payload", "C:/Temp/payload.exe", false);

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "targetPath".to_string(),
            Value::String("C:/Temp/payload.exe".to_string()),
        );
        insert_artifact(
            &conn,
            "artifact-download",
            "BrowserDownload",
            Some("browser-db"),
            attrs,
        );

        TimelineRepo::new(&conn)
            .insert_batch_with_case(
                &[TimelineEvent {
                    id: TimelineEventId("timeline-download".to_string()),
                    source_object_id: "file-payload".to_string(),
                    event_type: "FILE_CREATED".to_string(),
                    timestamp: Utc::now(),
                    title: "payload.exe created".to_string(),
                    description: "download landed".to_string(),
                    parser_id: Some("timeline.macb".to_string()),
                    parser_version: Some("1.0.0".to_string()),
                    confidence: Some(0.8),
                    source_attribution: Some("created_at".to_string()),
                    attrs: BTreeMap::new(),
                }],
                "case-1",
            )
            .unwrap();

        let snapshot = get_correlation_snapshot(&conn).unwrap();
        let cluster = snapshot
            .clusters
            .iter()
            .find(|item| item.id == "cluster:rules:file-payload")
            .unwrap();

        assert_eq!(cluster.timeline_count, 1);
        assert!(cluster
            .edge_ids
            .iter()
            .any(|item| item.contains("rule-timeline")));
        assert!(snapshot.edges.iter().any(|edge| {
            edge.id.contains("rule-timeline")
                && edge.to_node_id == "file:file-payload"
                && edge.kind == CorrelationEdgeKindDto::TemporalContext
        }));
    }

    #[test]
    fn correlation_snapshot_matches_jumplist_target_path_to_file() {
        let conn = setup_case_db();
        insert_file(
            &conn,
            "file-report",
            "C:/Users/Admin/Documents/report.docx",
            false,
        );

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "target_path".to_string(),
            Value::String("C:/Users/Admin/Documents/report.docx".to_string()),
        );
        insert_artifact(
            &conn,
            "artifact-jumplist",
            "JumpList",
            Some("jumplist-file"),
            attrs,
        );

        let snapshot = get_correlation_snapshot(&conn).unwrap();

        assert!(snapshot.edges.iter().any(|edge| {
            edge.kind == CorrelationEdgeKindDto::PathMatch
                && edge.from_node_id == "artifact:artifact-jumplist"
                && edge.to_node_id == "file:file-report"
        }));
    }

    #[test]
    fn correlation_snapshot_adds_proximity_timeline_signal_for_browser_download() {
        let conn = setup_case_db();
        insert_file(&conn, "file-payload", "C:/Temp/payload.exe", false);
        insert_file(
            &conn,
            "file-history",
            "C:/Users/Admin/AppData/Local/Edge/User Data/Default/History",
            false,
        );

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "targetPath".to_string(),
            Value::String("C:/Temp/payload.exe".to_string()),
        );
        attrs.insert(
            "startTime".to_string(),
            Value::String("2026-06-12T10:00:00Z".to_string()),
        );
        insert_artifact(
            &conn,
            "artifact-download-proximity",
            "BrowserDownload",
            Some("file-history"),
            attrs,
        );

        TimelineRepo::new(&conn)
            .insert_batch_with_case(
                &[TimelineEvent {
                    id: TimelineEventId("timeline-near-download".to_string()),
                    source_object_id: "other-file".to_string(),
                    event_type: "FILE_CREATED".to_string(),
                    timestamp: chrono::DateTime::parse_from_rfc3339("2026-06-12T10:05:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                    title: "payload created".to_string(),
                    description: "nearby timeline".to_string(),
                    parser_id: Some("timeline.macb".to_string()),
                    parser_version: Some("1.0.0".to_string()),
                    confidence: Some(0.75),
                    source_attribution: Some("C:/Temp/payload.exe".to_string()),
                    attrs: BTreeMap::new(),
                }],
                "case-1",
            )
            .unwrap();

        let snapshot = get_correlation_snapshot(&conn).unwrap();
        let lead = snapshot
            .leads
            .iter()
            .find(|item| item.id == "lead:rules:file-payload")
            .unwrap();

        assert!(lead
            .match_signals
            .iter()
            .any(|item| item.contains("邻近时间线命中 FILE_CREATED")));
    }

    #[test]
    fn correlation_snapshot_adds_proximity_timeline_signal_for_email_message() {
        let conn = setup_case_db();
        insert_file(&conn, "file-triage", "C:/Cases/triage.csv", false);
        insert_file(
            &conn,
            "file-mail",
            "C:/Users/Admin/Documents/incident-response.eml",
            false,
        );

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "attachments".to_string(),
            Value::Array(vec![Value::String("triage.csv".to_string())]),
        );
        attrs.insert(
            "subject".to_string(),
            Value::String("Initial triage notes".to_string()),
        );
        attrs.insert(
            "sentAt".to_string(),
            Value::String("2026-06-12T11:00:00Z".to_string()),
        );
        insert_artifact(
            &conn,
            "artifact-email-proximity",
            "EmailMessage",
            Some("file-mail"),
            attrs,
        );

        TimelineRepo::new(&conn)
            .insert_batch_with_case(
                &[TimelineEvent {
                    id: TimelineEventId("timeline-near-email".to_string()),
                    source_object_id: "other-file".to_string(),
                    event_type: "REPORT_UPDATED".to_string(),
                    timestamp: chrono::DateTime::parse_from_rfc3339("2026-06-12T11:10:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                    title: "Initial triage notes".to_string(),
                    description: "triage.csv refreshed".to_string(),
                    parser_id: Some("timeline.note".to_string()),
                    parser_version: Some("1.0.0".to_string()),
                    confidence: Some(0.72),
                    source_attribution: Some("C:/Cases/triage.csv".to_string()),
                    attrs: BTreeMap::new(),
                }],
                "case-1",
            )
            .unwrap();

        let snapshot = get_correlation_snapshot(&conn).unwrap();
        let lead = snapshot
            .leads
            .iter()
            .find(|item| item.id == "lead:rules:file-triage")
            .unwrap();

        assert!(lead
            .match_signals
            .iter()
            .any(|item| item.contains("邻近时间线命中 REPORT_UPDATED")));
    }

    #[test]
    fn correlation_snapshot_adds_proximity_timeline_signal_for_browser_history() {
        let conn = setup_case_db();
        insert_file(
            &conn,
            "file-browser-cache",
            "C:/Users/Admin/AppData/Local/Edge/User Data/Default/History",
            false,
        );
        insert_file(
            &conn,
            "file-report",
            "C:/Cases/browser-incident-report.docx",
            false,
        );

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "url".to_string(),
            Value::String("https://intranet.local/reports/browser-incident-report".to_string()),
        );
        attrs.insert(
            "title".to_string(),
            Value::String("browser-incident-report.docx draft".to_string()),
        );
        attrs.insert(
            "visitTime".to_string(),
            Value::String("2026-06-12T12:00:00Z".to_string()),
        );
        insert_artifact(
            &conn,
            "artifact-browser-history-proximity",
            "BrowserHistory",
            Some("file-browser-cache"),
            attrs,
        );

        TimelineRepo::new(&conn)
            .insert_batch_with_case(
                &[TimelineEvent {
                    id: TimelineEventId("timeline-near-browser-history".to_string()),
                    source_object_id: "other-file".to_string(),
                    event_type: "REPORT_OPENED".to_string(),
                    timestamp: chrono::DateTime::parse_from_rfc3339("2026-06-12T12:15:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                    title: "browser-incident-report.docx draft".to_string(),
                    description: "C:/Cases/browser-incident-report.docx opened".to_string(),
                    parser_id: Some("timeline.note".to_string()),
                    parser_version: Some("1.0.0".to_string()),
                    confidence: Some(0.78),
                    source_attribution: Some("C:/Cases/browser-incident-report.docx".to_string()),
                    attrs: BTreeMap::new(),
                }],
                "case-1",
            )
            .unwrap();

        let snapshot = get_correlation_snapshot(&conn).unwrap();
        let lead = snapshot
            .leads
            .iter()
            .find(|item| item.id == "lead:rules:file-report")
            .unwrap();

        assert_eq!(lead.confidence, CorrelationConfidenceDto::Strong);
        assert!(lead
            .match_signals
            .iter()
            .any(|item| item.contains("BrowserHistory 标题或 URL 命中文件名")));
        assert!(lead
            .match_signals
            .iter()
            .any(|item| item.contains("邻近时间线命中 REPORT_OPENED")));
    }
}
