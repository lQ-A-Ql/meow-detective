use artifacts_windows::registry::RegistrySnapshot;
use evidence_access::file_service::SourceReadContext;
use persistence_source::fjall::{CatalogRecordStore, FileEntryRecord, ScanDirection};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn run() {
    let (active, source, _, output) = super::analysis_diagnostic::open_diagnostic();
    let report = active
        .with_conn(|connection| {
            let sql = app_services::source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                &source.id,
            )?;
            let entries = selected_entries(&sql, &source.id.0);
            let mut reader = SourceReadContext::new(
                &sql,
                connection,
                &active.case_root,
                &active.meta.id,
                &source.id,
            );
            let mut bytes_by_path = BTreeMap::new();
            let mut inputs = Vec::new();
            for entry in &entries {
                assert!(entry.size.unwrap_or(0) <= 256 * 1024 * 1024);
                let bytes = reader
                    .read_file_header_by_id(
                        &domain::FileEntryId(entry.id.clone()),
                        entry.size.unwrap_or(0) as usize + 1,
                    )
                    .unwrap();
                let path = format!("/{}", entry.path.trim_start_matches('/').to_lowercase());
                let parser = if path.ends_with("/var/log/btmp") {
                    Some(artifacts_linux::parse_wtmp(&bytes).map(|rows| rows.len()))
                } else if path.ends_with("/var/log/lastlog") {
                    Some(artifacts_linux::parse_lastlog(&bytes).map(|rows| rows.len()))
                } else {
                    None
                };
                inputs.push(json!({
                    "fileId":entry.id, "path":entry.path, "catalogBytes":entry.size,
                    "deleted":entry.deleted,
                    "readBytes":bytes.len(), "allZero":bytes.iter().all(|byte| *byte == 0),
                    "leadingNonzeroByte":bytes.iter().position(|byte| *byte != 0),
                    "firstBytesHex":hex::encode(&bytes[..bytes.len().min(16)]),
                    "utf8Valid":std::str::from_utf8(&bytes).is_ok(),
                    "sha256":hex::encode(Sha256::digest(&bytes)),
                    "parser":parser.map(|result| match result {
                        Ok(rows) => json!({"state":"parsed", "records":rows}),
                        Err(error) => json!({"state":"failed", "reason":error.to_string()}),
                    })
                }));
                if path.ends_with("/var/log/btmp") && bytes.starts_with(b"\n") {
                    let diagnostic = match artifacts_linux::parse_wtmp(&bytes[1..]) {
                        Ok(rows) => json!({"state":"parsed", "records":rows.len()}),
                        Err(error) => json!({"state":"failed", "reason":error.to_string()}),
                    };
                    inputs.last_mut().unwrap().as_object_mut().unwrap()
                        .insert("parserWithoutLeadingLineFeed".into(), diagnostic);
                }
                if path.contains("/systemd/") && path.rsplit('/').next().unwrap().starts_with("dracut-") {
                    inputs.last_mut().unwrap().as_object_mut().unwrap()
                        .insert("unitInput".into(), json!(std::str::from_utf8(&bytes).ok()));
                }
                if path.ends_with("/var/log/lastlog") {
                    let mut supported = Vec::new();
                    for record_bytes in [292, 296] {
                        let first_content = bytes
                            .chunks_exact(record_bytes)
                            .position(|slot| slot.iter().any(|byte| *byte != 0));
                        let result = first_content.map(|slot| {
                            artifacts_linux::parse_lastlog(&bytes[slot * record_bytes..])
                                .map(|rows| rows.len())
                        });
                        supported.push(json!({"recordBytes":record_bytes,
                            "leadingSlotsZero":bytes[..bytes.len().min(8 * record_bytes)].iter().all(|byte| *byte == 0),
                            "firstContentSlot":first_content,
                            "parserStartingAtFirstContent":result.map(|result| match result {
                                Ok(rows) => json!({"state":"parsed", "records":rows}),
                                Err(error) => json!({"state":"failed", "reason":error.to_string()}),
                            })}));
                    }
                    inputs.last_mut().unwrap().as_object_mut().unwrap()
                        .insert("supportedLayoutObservations".into(), json!(supported));
                }
                bytes_by_path.insert(path, bytes);
            }
            let mut hives = Vec::new();
            for entry in &entries {
                let path = format!("/{}", entry.path.trim_start_matches('/').to_lowercase());
                if !["/system", "/sam", "/security"]
                    .iter()
                    .any(|suffix| path.ends_with(suffix))
                {
                    continue;
                }
                let bytes = bytes_by_path.get(&path).unwrap();
                let snapshot = RegistrySnapshot::from_owned(
                    bytes.clone(),
                    bytes_by_path
                        .get(&format!("{path}.log1"))
                        .map(Vec::as_slice),
                    bytes_by_path
                        .get(&format!("{path}.log2"))
                        .map(Vec::as_slice),
                )
                .unwrap();
                hives.push(json!({
                    "path":entry.path, "catalogBytes":entry.size,
                    "rawReadBytes":bytes.len(), "preparedBytes":snapshot.bytes().len(),
                    "logsApplied":snapshot.logs_applied(),
                    "replayReport":snapshot.report().map(|report| json!({
                        "state":format!("{:?}", report.state),
                        "basePrimary":report.base_primary, "baseSecondary":report.base_secondary,
                        "resultSequence":report.result_sequence,
                        "appliedEntries":report.applied_entries, "appliedPages":report.applied_pages,
                        "diagnostics":report.diagnostics.iter().map(|error| error.code()).collect::<Vec<_>>()
                    })),
                    "registryViewValid":snapshot.view().is_ok()
                }));
            }
            Ok(json!({"caseId":active.meta.id.0, "sourceId":source.id.0,
                "inputs":inputs, "registrySnapshots":hives}))
        })
        .unwrap();
    let path = output.join("input-diagnostic.json");
    super::write_report(&path, &report);
    eprintln!("INPUT_DIAGNOSTIC_REPORT {}", path.display());
}

fn selected_entries(connection: &rusqlite::Connection, source: &str) -> Vec<FileEntryRecord> {
    let store = persistence_source::detail::require_source_store(connection).unwrap();
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut cursor = None;
    let mut entries = Vec::new();
    loop {
        let page = catalog
            .list_all(
                &snapshot,
                source,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .unwrap();
        for entry in page.records {
            let path = format!("/{}", entry.path.trim_start_matches('/').to_lowercase());
            let hive = ["system", "sam", "security"].iter().any(|name| {
                let suffix = format!("/windows/system32/config/{name}");
                path.ends_with(&suffix)
                    || path.ends_with(&format!("{suffix}.log1"))
                    || path.ends_with(&format!("{suffix}.log2"))
            });
            let prefetch = path.ends_with("/prefetch/msedgewebview2.exe-593afd87.pf")
                || path.ends_with("/prefetch/tabtip.exe-9740ca06.pf");
            let dracut_unit = path.contains("/systemd/")
                && path.rsplit('/').next().unwrap().starts_with("dracut-")
                && path.ends_with(".service");
            let dracut_target = path.contains("/dracut/modules.d/") && path.ends_with(".service");
            let browser = (path.contains("/google/chrome/user data/")
                || path.contains("/microsoft/edge/user data/"))
                && ["/history", "/cookies", "/login data"]
                    .iter()
                    .any(|suffix| path.ends_with(suffix));
            if prefetch
                || (!entry.deleted
                    && (hive
                        || browser
                        || dracut_unit
                        || dracut_target
                        || path.ends_with("/var/log/btmp")
                        || path.ends_with("/var/log/lastlog")))
            {
                entries.push(entry);
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            assert!(!entries.is_empty(), "selected sample inputs must exist");
            return entries;
        }
    }
}
