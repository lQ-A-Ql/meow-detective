use evidence_access::file_service::SourceReadContext;
use persistence_source::fjall::{CatalogEntryType, FileEntryRecord, ScanDirection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[path = "real_sample_warning_parsers.rs"]
mod parsers;

pub(super) fn run() {
    let (active, source, _, output) = super::analysis_diagnostic::open_diagnostic();
    let paths: Vec<String> = serde_json::from_slice(
        &std::fs::read(
            std::env::var_os("FORENSICS_DIAGNOSTIC_PATHS").expect("set FORENSICS_DIAGNOSTIC_PATHS"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(paths.len() <= 64);
    let paths = paths
        .into_iter()
        .map(|path| path.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let report = active
        .with_conn(|connection| {
            let handle = app_services::source_db::open_registered_fjall_source_read_only(
                &active.case_root,
                &source.id,
            )
            .unwrap();
            let snapshot = handle.snapshot();
            let mut after: Option<(String, String)> = None;
            let mut selected = Vec::new();
            let mut exclusions = BTreeMap::<String, u64>::new();
            let mut evtx = BTreeMap::<String, Vec<Value>>::new();
            let mut scanned = 0u64;
            let mut header_window = Vec::new();
            for _ in 0..32 {
                let page = handle
                    .catalog_files_by_path(
                        &snapshot,
                        &source.id.0,
                        after
                            .as_ref()
                            .map(|(path, id)| (path.as_str(), id.as_str())),
                        128,
                    )
                    .unwrap();
                for entry in page.records {
                    after = Some((entry.path.clone(), entry.id.clone()));
                    *exclusions
                        .entry(header_reason(&entry).to_string())
                        .or_default() += 1;
                    if header_window.len() < 12 || scanned >= 4084 {
                        header_window.push(json!({
                            "position": scanned,
                            "entry": entry,
                            "selection": header_reason(&entry),
                        }));
                    }
                    scanned += 1;
                }
                if page.next_cursor.is_none() {
                    break;
                }
            }
            let mut cursor = None;
            scanned = 0;
            loop {
                let page = handle
                    .catalog_list_all(
                        &snapshot,
                        &source.id.0,
                        cursor.as_deref(),
                        ScanDirection::Forward,
                        1024,
                    )
                    .unwrap();
                for entry in page.records {
                    scanned += 1;
                    if entry.entry_type != CatalogEntryType::File {
                        continue;
                    }
                    if entry.path.to_ascii_lowercase().ends_with(".evtx") {
                        let result =
                            artifacts_windows::extract_boot_shutdown_events_from_json_records(
                                &[],
                                &entry.path,
                            );
                        let state = if result.is_ok() {
                            "accepted"
                        } else {
                            "rejected"
                        };
                        evtx.entry(state.into()).or_default().push(json!({
                            "id": entry.id,
                            "path": entry.path,
                            "deleted": entry.deleted,
                            "reason": result.err().map(|error| error.to_string()),
                        }));
                    }
                    if paths
                        .iter()
                        .any(|path| entry.path.to_ascii_lowercase().ends_with(path))
                    {
                        selected.push(entry);
                        assert!(
                            selected.len() <= 128,
                            "diagnostic path selection is too broad"
                        );
                    }
                }
                if page.next_cursor.is_none() {
                    break;
                }
                cursor = page.next_cursor;
            }
            let sql = app_services::source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                &source.id,
            )?;
            let mut reader = SourceReadContext::new(
                &sql,
                connection,
                &active.case_root,
                &active.meta.id,
                &source.id,
            );
            let mut technical_entries = BTreeMap::new();
            let inputs = selected
                .iter()
                .map(|entry| {
                    technical_entries.insert(entry.id.clone(), entry.clone());
                    let mut report = input_report(entry, &mut reader, &output);
                    let mut parents = Vec::new();
                    let mut parent = entry.parent_id.clone();
                    while let Some(id) = parent {
                        let record = handle.catalog_get(&snapshot, &source.id.0, &id).unwrap();
                        parent = record.as_ref().and_then(|record| record.parent_id.clone());
                        if let Some(record) = &record {
                            technical_entries.insert(record.id.clone(), record.clone());
                        }
                        parents.push(json!(record));
                        if parents.len() >= 12 {
                            break;
                        }
                    }
                    report["parents"] = json!(parents);
                    report
                })
                .collect::<Vec<_>>();
            let technical = technical_entries
                .values()
                .filter(|entry| {
                    entry.id.starts_with("mft:") && entry.entry_type == CatalogEntryType::File
                })
                .map(|entry| {
                    let global = app_services::source_db::GlobalFileId::new(
                        source.id.clone(),
                        domain::FileEntryId(entry.id.clone()),
                    )
                    .encode();
                    let result = evidence_access::file_service::inspect_ntfs_file_for_case(
                        &std::sync::Arc::default(),
                        connection,
                        &active.case_root,
                        &active.meta.id,
                        &global.0,
                    );
                    match result {
                        Ok(file) => json!({
                            "id": entry.id,
                            "sequenceNumber": file.sequence_number,
                            "flags": file.flags,
                            "parentReference": file.parent_reference,
                            "attributes": file.attributes.iter().map(|attribute| json!({
                                "type": attribute.attribute_type,
                                "name": attribute.name,
                                "rawHeaderHex": hex::encode(&attribute.raw[..attribute.raw.len().min(96)]),
                            })).collect::<Vec<_>>(),
                        }),
                        Err(error) => json!({"id": entry.id, "error": error.to_string()}),
                    }
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "caseId": active.meta.id.0,
                "sourceId": source.id.0,
                "catalogRecordsScanned": scanned,
                "headerWindowExclusions": exclusions,
                "headerWindowExamples": header_window,
                "evtxPathValidation": evtx,
                "inputs": inputs,
                "technical": technical,
            }))
        })
        .unwrap();
    super::write_report(&output.join("warning-diagnostic.json"), &report);
}

fn header_reason(entry: &FileEntryRecord) -> &'static str {
    if entry.deleted {
        "deleted"
    } else if entry.encrypted == Some(true) {
        "encrypted"
    } else if entry.size.unwrap_or(0) == 0 {
        "empty"
    } else if entry.name.starts_with('$') {
        "metadataName"
    } else {
        "eligible"
    }
}

fn input_report(
    entry: &FileEntryRecord,
    reader: &mut SourceReadContext<'_>,
    output: &std::path::Path,
) -> Value {
    assert_eq!(entry.entry_type, CatalogEntryType::File);
    let limit = entry.size.unwrap_or(0).saturating_add(1).min(128 * 1024) as usize;
    let result = reader.read_file_header_by_id(&domain::FileEntryId(entry.id.clone()), limit);
    let content = match result {
        Ok(bytes) => {
            let saved_input = if entry.name.to_ascii_lowercase().ends_with(".pf")
                && Some(bytes.len() as u64) == entry.size
            {
                use std::io::Write;
                let path = output.join(format!("{}.prefetch.bin", uuid::Uuid::new_v4()));
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .write_all(&bytes)
                    .unwrap();
                Some(path)
            } else {
                None
            };
            json!({
                "readBytes": bytes.len(),
                "complete": Some(bytes.len() as u64) == entry.size,
                "leadingHex": hex::encode(&bytes[..bytes.len().min(32)]),
                "allZero": bytes.iter().all(|byte| *byte == 0),
                "sha256": hex::encode(Sha256::digest(&bytes)),
                "parser": parsers::observe(entry, &bytes),
                "savedInput": saved_input,
            })
        }
        Err(error) => json!({"error": error.to_string()}),
    };
    json!({"entry": entry, "content": content})
}
