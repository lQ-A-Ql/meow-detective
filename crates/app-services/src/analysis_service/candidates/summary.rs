use super::{
    discover_evidence_candidates, evidence_category_defs, EvidenceCandidate, EvidenceCategoryDef,
};
use crate::analysis_service::classification::metadata_category_stats;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::platforms::evidence_summary_category_allowed;
use crate::analysis_service::provenance::unknown_provenance;
use domain::DataSourcePlatform;
use rusqlite::Connection;
use std::collections::HashMap;
use transport::dto::analysis::AnalysisProvenanceDto;
use transport::dto::{
    AnalysisParseStatusDto, EvidenceCategoryDto, EvidenceClassificationSummaryDto,
    EvidenceClassificationTotalsDto, EvidenceSourceDto,
};

const MAX_EVIDENCE_SOURCES_PER_CATEGORY: usize = 8;

pub fn get_evidence_classification_summary(
    conn: &Connection,
    platform: DataSourcePlatform,
) -> Result<EvidenceClassificationSummaryDto, AnalysisServiceError> {
    let generated_at = chrono::Utc::now().to_rfc3339();
    let candidates = discover_evidence_candidates(conn)?;
    let artifact_counts = artifact_counts_by_family(conn)?;
    let source_artifact_counts = artifact_counts_by_source(conn)?;
    let file_type_totals = metadata_category_stats(conn)?;
    let file_type_count = file_type_totals.values().map(|(count, _)| *count).sum();
    let file_type_size = file_type_totals.values().map(|(_, size)| *size).sum();
    let categories = build_categories(
        platform,
        &candidates,
        &artifact_counts,
        &source_artifact_counts,
        file_type_count,
        file_type_size,
        &generated_at,
    )?;
    let totals = build_totals(&categories);
    let status = summary_status(&categories);
    let warnings = if totals.candidate_file_count == 0 {
        vec!["未发现证据族候选文件；请确认数据源已导入且文件树可用。".to_string()]
    } else {
        Vec::new()
    };

    Ok(EvidenceClassificationSummaryDto {
        status,
        categories,
        totals,
        generated_at,
        warnings,
    })
}

fn build_categories(
    platform: DataSourcePlatform,
    candidates: &HashMap<String, Vec<EvidenceCandidate>>,
    artifact_counts: &HashMap<String, u64>,
    source_artifact_counts: &HashMap<String, u64>,
    file_type_count: u64,
    file_type_size: u64,
    generated_at: &str,
) -> Result<Vec<EvidenceCategoryDto>, AnalysisServiceError> {
    let mut categories = Vec::new();
    for definition in evidence_category_defs() {
        if !evidence_summary_category_allowed(platform, definition.category)? {
            continue;
        }
        categories.push(if definition.category == "FileTypeInventory" {
            build_file_type_category(definition, file_type_count, file_type_size, generated_at)
        } else {
            build_evidence_category(
                definition,
                candidates
                    .get(definition.category)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                artifact_counts,
                source_artifact_counts,
                generated_at,
            )
        });
    }
    Ok(categories)
}

fn build_file_type_category(
    definition: &EvidenceCategoryDef,
    file_count: u64,
    total_size: u64,
    generated_at: &str,
) -> EvidenceCategoryDto {
    let status = if file_count > 0 {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotFound
    };
    EvidenceCategoryDto {
        category: definition.category.to_string(),
        display_name: definition.display_name.to_string(),
        status: status.clone(),
        file_count,
        total_size,
        artifact_count: 0,
        confidence: if file_count > 0 { 0.75 } else { 0.0 },
        sources: Vec::new(),
        warnings: if file_count > 0 {
            vec!["文件类型清单来自 metadata-only 分类；未读取文件正文。".to_string()]
        } else {
            Vec::new()
        },
        provenance: vec![unknown_provenance(
            definition.parser,
            generated_at,
            status,
            vec!["metadata aggregate from file_entries".to_string()],
        )],
    }
}

fn build_evidence_category(
    definition: &EvidenceCategoryDef,
    candidates: &[EvidenceCandidate],
    artifact_counts: &HashMap<String, u64>,
    source_artifact_counts: &HashMap<String, u64>,
    generated_at: &str,
) -> EvidenceCategoryDto {
    let file_count = candidates.len() as u64;
    let total_size = candidates.iter().map(|candidate| candidate.size).sum();
    let artifact_count = definition
        .artifact_families
        .iter()
        .filter_map(|family| artifact_counts.get(*family))
        .sum();
    let parsed_source_count = candidates
        .iter()
        .filter(|candidate| source_artifact_counts.contains_key(&candidate.file_id.0))
        .count() as u64;
    let sources = build_sources(candidates, source_artifact_counts);
    let warnings = build_category_warnings(file_count, artifact_count, sources.len());
    let provenance = build_provenance(candidates, &sources, generated_at);

    EvidenceCategoryDto {
        category: definition.category.to_string(),
        display_name: definition.display_name.to_string(),
        status: evidence_category_status(file_count, artifact_count, parsed_source_count),
        file_count,
        total_size,
        artifact_count,
        confidence: evidence_confidence(file_count, artifact_count),
        sources,
        warnings,
        provenance,
    }
}

fn build_sources(
    candidates: &[EvidenceCandidate],
    source_artifact_counts: &HashMap<String, u64>,
) -> Vec<EvidenceSourceDto> {
    candidates
        .iter()
        .take(MAX_EVIDENCE_SOURCES_PER_CATEGORY)
        .map(|candidate| {
            let artifact_count = source_artifact_counts
                .get(&candidate.file_id.0)
                .copied()
                .unwrap_or(0);
            EvidenceSourceDto {
                file_id: candidate.file_id.0.clone(),
                path: candidate.path.clone(),
                size: candidate.size,
                evidence_kind: candidate.evidence_kind.clone(),
                parser: candidate.parser.clone(),
                status: if artifact_count > 0 {
                    AnalysisParseStatusDto::Parsed
                } else {
                    AnalysisParseStatusDto::CandidateFound
                },
                artifact_count,
                warnings: Vec::new(),
            }
        })
        .collect()
}

fn build_category_warnings(
    file_count: u64,
    artifact_count: u64,
    displayed_source_count: usize,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if file_count > displayed_source_count as u64 {
        warnings.push(format!(
            "仅展示前 {} 个候选来源；该证据族共 {} 个候选文件。",
            displayed_source_count, file_count
        ));
    }
    if file_count > 0 && artifact_count == 0 {
        warnings
            .push("已发现候选文件；尚未运行证据分类解析或当前 parser 不支持该证据族。".to_string());
    }
    warnings
}

fn build_provenance(
    candidates: &[EvidenceCandidate],
    sources: &[EvidenceSourceDto],
    generated_at: &str,
) -> Vec<AnalysisProvenanceDto> {
    candidates
        .iter()
        .zip(sources)
        .map(|(candidate, source)| AnalysisProvenanceDto {
            data_source_id: candidate.data_source_id.clone(),
            artifact_path: source.path.clone(),
            parser: source.parser.clone(),
            parsed_at: generated_at.to_string(),
            status: source.status.clone(),
            warnings: source.warnings.clone(),
        })
        .collect()
}

fn artifact_counts_by_family(
    conn: &Connection,
) -> Result<HashMap<String, u64>, AnalysisServiceError> {
    let mut statement =
        conn.prepare("SELECT artifact_type, COUNT(*) FROM artifacts GROUP BY artifact_type")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
    })?;
    let mut counts = HashMap::new();
    for row in rows {
        let (family, count) = row?;
        counts.insert(family, count);
    }
    Ok(counts)
}

fn artifact_counts_by_source(
    conn: &Connection,
) -> Result<HashMap<String, u64>, AnalysisServiceError> {
    let mut statement = conn.prepare(
        "SELECT source_object_id, COUNT(*)
         FROM artifacts
         WHERE source_object_id IS NOT NULL
         GROUP BY source_object_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
    })?;
    let mut counts = HashMap::new();
    for row in rows {
        let (source_id, count) = row?;
        counts.insert(source_id, count);
    }
    Ok(counts)
}

fn build_totals(categories: &[EvidenceCategoryDto]) -> EvidenceClassificationTotalsDto {
    EvidenceClassificationTotalsDto {
        category_count: categories.len() as u64,
        candidate_file_count: categories
            .iter()
            .filter(|category| category.category != "FileTypeInventory")
            .map(|category| category.file_count)
            .sum(),
        total_size: categories
            .iter()
            .filter(|category| category.category != "FileTypeInventory")
            .map(|category| category.total_size)
            .sum(),
        artifact_count: categories
            .iter()
            .map(|category| category.artifact_count)
            .sum(),
    }
}

fn summary_status(categories: &[EvidenceCategoryDto]) -> AnalysisParseStatusDto {
    if categories
        .iter()
        .any(|category| category.status == AnalysisParseStatusDto::Failed)
    {
        AnalysisParseStatusDto::Partial
    } else if categories
        .iter()
        .any(|category| category.status == AnalysisParseStatusDto::Parsed)
    {
        AnalysisParseStatusDto::Parsed
    } else if categories
        .iter()
        .any(|category| category.status == AnalysisParseStatusDto::CandidateFound)
    {
        AnalysisParseStatusDto::CandidateFound
    } else {
        AnalysisParseStatusDto::NotFound
    }
}

fn evidence_category_status(
    file_count: u64,
    artifact_count: u64,
    parsed_source_count: u64,
) -> AnalysisParseStatusDto {
    if file_count == 0 {
        AnalysisParseStatusDto::NotFound
    } else if artifact_count == 0 {
        AnalysisParseStatusDto::CandidateFound
    } else if parsed_source_count > 0 && parsed_source_count < file_count {
        AnalysisParseStatusDto::Partial
    } else {
        AnalysisParseStatusDto::Parsed
    }
}

fn evidence_confidence(file_count: u64, artifact_count: u64) -> f32 {
    if artifact_count > 0 {
        0.95
    } else if file_count > 0 {
        0.65
    } else {
        0.0
    }
}
