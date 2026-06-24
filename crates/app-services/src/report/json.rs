use super::{
    current_analysis, current_correlation, current_governance, persist_report_record,
    prepare_report_output, write_report_atomically, RawExportBundle, ReportCorrelation,
    ReportError, ReportGovernance,
};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use reports::JsonExporter;
use rusqlite::Connection;
use sha2::Digest;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use transport::commands::ExportScopeDto;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Public JSON export
// ---------------------------------------------------------------------------

pub fn generate_json_export(
    conn: &Connection,
    case_id: &str,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    let events = if scope.full_timeline {
        TimelineRepo::new(conn).query(0, 500)?
    } else {
        Vec::new()
    };
    let artifacts = ArtifactRepo::new(conn).list_by_family(None)?;
    let analysis = current_analysis(conn)?;
    let governance = current_governance(conn, case_id)?;
    let correlation = current_correlation(conn)?;
    let summary = crate::analysis_service::generate_analysis_summary(
        &analysis.system_info,
        &analysis.classifications,
    );
    let system_info = if scope.registry {
        Some(&analysis.system_info)
    } else {
        None
    };
    let classifications = if scope.file_system_metadata {
        analysis.classifications.as_slice()
    } else {
        &[]
    };
    let json_val = serde_json::json!({
        "timeline_events": events.iter().map(|e| serde_json::json!({
            "id": e.id.0,
            "sourceObjectId": e.source_object_id,
            "type": e.event_type,
            "ts": e.timestamp.to_rfc3339(),
            "title": e.title,
            "description": e.description,
            "parserId": e.parser_id,
            "parserVersion": e.parser_version,
            "confidence": e.confidence,
            "sourceAttribution": e.source_attribution,
        })).collect::<Vec<_>>(),
        "artifacts": artifacts.iter().map(|artifact| serde_json::json!({
            "id": artifact.id.0,
            "artifactType": artifact.family,
            "title": artifact.title,
            "summary": artifact.summary,
            "sourceObjectId": artifact.source_object_id.as_ref().map(|id| id.0.as_str()),
            "extractorId": artifact.extractor_id,
            "extractorVersion": artifact.extractor_version,
            "confidence": artifact.confidence,
            "sourceAttribution": artifact.source_attribution,
            "createdAt": artifact.created_at.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "scope": scope,
        "warnings": serde_json::Value::Array(Vec::new()),
        "analysis": {
            "systemInfo": system_info,
            "classifications": classifications,
            "summary": summary,
        },
        "governance": governance_json_section(&governance),
        "correlation": correlation_json_section(&correlation),
    });

    let file_name = format!("export-{}.json", Uuid::new_v4());
    let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
    let raw_bundle = if scope.raw_file_extraction {
        Some(export_raw_file_bundle(
            conn,
            output_dir,
            case_id,
            &file_name,
            scope.overwrite,
        )?)
    } else {
        None
    };
    let warnings = super::report_warnings(conn, case_id, scope, raw_bundle.as_ref());
    write_report_atomically(&path, scope.overwrite, |file| {
        let mut payload = json_val;
        if let Some(bundle) = &raw_bundle {
            payload["rawExport"] = serde_json::json!({
                "bundleDirectory": bundle.bundle_dir_name,
                "manifestFile": bundle.manifest_file_name,
                "hashesFile": bundle.hashes_file_name,
                "exportedCount": bundle.exported_count,
            });
        }
        payload["warnings"] =
            serde_json::to_value(&warnings).map_err(|e| ReportError::Other(e.to_string()))?;
        JsonExporter::export(file, &payload).map_err(|e| ReportError::Other(e.to_string()))
    })?;

    persist_report_record(conn, case_id, "report-summary", &file_name, "completed")?;
    Ok(file_name)
}

// ---------------------------------------------------------------------------
// JSON section helpers
// ---------------------------------------------------------------------------

pub(crate) fn correlation_json_section(correlation: &ReportCorrelation) -> serde_json::Value {
    serde_json::json!({
        "generatedAt": correlation.snapshot.generated_at,
        "leadCount": correlation.snapshot.lead_count,
        "clusterCount": correlation.snapshot.cluster_count,
        "nodeCount": correlation.snapshot.node_count,
        "edgeCount": correlation.snapshot.edge_count,
        "familyCoverage": correlation.snapshot.family_coverage,
        "leads": correlation.snapshot.leads.iter().map(|lead| serde_json::json!({
            "id": lead.id,
            "title": lead.title,
            "summary": lead.summary,
            "confidence": lead.confidence,
            "families": lead.families,
            "primaryFileId": lead.primary_file_id,
            "supportingNodeIds": lead.supporting_node_ids,
            "matchSignals": lead.match_signals,
            "jumps": lead.jumps,
            "provenance": lead.provenance,
            "caveats": lead.caveats,
        })).collect::<Vec<_>>(),
    })
}

pub(crate) fn governance_json_section(governance: &ReportGovernance) -> serde_json::Value {
    let snapshot = &governance.snapshot;
    serde_json::json!({
        "generatedAt": snapshot.generated_at,
        "factSources": snapshot.fact_sources,
        "runtimeResults": snapshot.runtime_results,
        "verificationChains": snapshot.verification_chains,
        "supportMatrix": snapshot.support_matrix,
        "supportMatrixEntries": snapshot.support_matrix_entries,
        "knownLimitations": snapshot.known_limitations,
        "benchmark": snapshot.benchmark,
        "security": snapshot.security,
        "errorTaxonomyEntries": snapshot.error_taxonomy_entries,
        "releaseGates": snapshot.release_gates,
        "releaseScorecard": snapshot.release_scorecard,
        "runtimeSignals": snapshot.runtime_signals,
    })
}

// ---------------------------------------------------------------------------
// Raw export bundle
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RawExportManifestEntry {
    file_id: String,
    data_source_id: String,
    relative_source_path: String,
    exported_relative_path: String,
    size: Option<u64>,
    sha256: Option<String>,
    deleted: bool,
    hidden: bool,
    system: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RawExportManifest {
    case_id: String,
    generated_from_report: String,
    exported_count: usize,
    files: Vec<RawExportManifestEntry>,
}

fn export_raw_file_bundle(
    conn: &Connection,
    output_dir: &Path,
    case_id: &str,
    report_file_name: &str,
    overwrite: bool,
) -> Result<RawExportBundle, ReportError> {
    let bundle_dir_name = bundle_dir_name_from_report(report_file_name);
    let bundle_dir = output_dir.join(&bundle_dir_name);
    prepare_bundle_directory(&bundle_dir, overwrite)?;

    let entries = collect_exportable_file_entries(conn)?;
    let export_root = bundle_dir.join("files");
    fs::create_dir_all(&export_root)?;

    let mut manifest_entries = Vec::new();
    let mut hash_lines = Vec::new();

    for entry in entries {
        let mut reader = match crate::file_service::open_file_content_by_id(conn, &entry.id) {
            Ok(reader) => reader,
            Err(_) => continue,
        };

        let safe_name = sanitize_bundle_component(&entry.name);
        let export_rel = PathBuf::from(entry.data_source_id.0.clone())
            .join(format!("{}-{}", entry.id.0, safe_name));
        let export_path = export_root.join(&export_rel);
        if let Some(parent) = export_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&export_path)?;

        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut total_bytes = 0u64;
        loop {
            let read = reader.read(&mut buffer).map_err(ReportError::Io)?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
            total_bytes = total_bytes.saturating_add(read as u64);
        }
        output.flush()?;
        output.sync_all()?;

        let sha256 = format!("{:x}", hasher.finalize());
        hash_lines.push(format!(
            "{}  {}",
            sha256,
            normalize_manifest_path(&PathBuf::from("files").join(&export_rel))
        ));
        manifest_entries.push(RawExportManifestEntry {
            file_id: entry.id.0.clone(),
            data_source_id: entry.data_source_id.0.clone(),
            relative_source_path: entry.path.clone(),
            exported_relative_path: normalize_manifest_path(
                &PathBuf::from("files").join(&export_rel),
            ),
            size: entry.size.or(Some(total_bytes)),
            sha256: Some(sha256),
            deleted: entry.deleted,
            hidden: entry.hidden,
            system: entry.system,
        });
    }

    let manifest = RawExportManifest {
        case_id: case_id.to_string(),
        generated_from_report: report_file_name.to_string(),
        exported_count: manifest_entries.len(),
        files: manifest_entries,
    };
    let manifest_file_name = "manifest.json".to_string();
    let hashes_file_name = "SHA256SUMS.txt".to_string();
    fs::write(
        bundle_dir.join(&manifest_file_name),
        serde_json::to_vec_pretty(&manifest).map_err(|e| ReportError::Other(e.to_string()))?,
    )?;
    fs::write(bundle_dir.join(&hashes_file_name), hash_lines.join("\n"))?;

    Ok(RawExportBundle {
        bundle_dir_name,
        manifest_file_name,
        hashes_file_name,
        exported_count: manifest.exported_count,
    })
}

fn collect_exportable_file_entries(
    conn: &Connection,
) -> Result<Vec<domain::FileEntry>, ReportError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries
             WHERE entry_type = 'file'
             ORDER BY data_source_id ASC, path ASC",
        )?;
    let rows = stmt.query_map([], |row| {
        let entry_type: String = row.get(5)?;
        Ok(domain::FileEntry {
            id: domain::FileEntryId(row.get::<_, String>(0)?),
            parent_id: row.get::<_, Option<String>>(1)?.map(domain::FileEntryId),
            data_source_id: domain::DataSourceId(row.get::<_, String>(2)?),
            path: row.get(3)?,
            name: row.get(4)?,
            entry_type: if entry_type.eq_ignore_ascii_case("directory") {
                domain::EntryType::Directory
            } else {
                domain::EntryType::File
            },
            size: row.get(6)?,
            ext: row.get(7)?,
            deleted: row.get::<_, i32>(8)? != 0,
            hidden: row.get::<_, i32>(9)? != 0,
            system: row.get::<_, i32>(10)? != 0,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: row.get(15)?,
        })
    })?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

fn bundle_dir_name_from_report(report_file_name: &str) -> String {
    let stem = Path::new(report_file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("export");
    format!("{stem}-bundle")
}

fn prepare_bundle_directory(bundle_dir: &Path, overwrite: bool) -> Result<(), ReportError> {
    if bundle_dir.exists() {
        if !overwrite {
            return Err(ReportError::Other(format!(
                "raw export bundle already exists: {} (set overwrite=true to replace it)",
                bundle_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("bundle")
            )));
        }
        fs::remove_dir_all(bundle_dir)?;
    }
    fs::create_dir_all(bundle_dir)?;
    Ok(())
}

pub(crate) fn sanitize_bundle_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if sanitized.trim().is_empty() {
        "file".to_string()
    } else {
        sanitized
    }
}

fn normalize_manifest_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
