use base64::{engine::general_purpose::STANDARD, Engine};
use evidence_access::file_service::SourceReadContext;
use persistence_source::fjall::{
    CatalogEntryType, CatalogRecordStore, FileEntryRecord, ScanDirection,
};
use serde_json::{json, Value};

const MAX_LOCAL_STATE_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn run() {
    let (active, source, _, output) = super::analysis_diagnostic::open_diagnostic();
    let reports = active
        .with_conn(|connection| {
            let sql = app_services::source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                &source.id,
            )?;
            let entries = local_state_entries(&sql, &source.id.0);
            let mut reader = SourceReadContext::new(
                &sql,
                connection,
                &active.case_root,
                &active.meta.id,
                &source.id,
            );
            let reports = entries
                .into_iter()
                .map(|entry| {
                    assert!(entry.size.is_some_and(|size| size <= MAX_LOCAL_STATE_BYTES as u64));
                    let bytes = reader
                        .read_file_header_by_id(
                            &domain::FileEntryId(entry.id.clone()),
                            MAX_LOCAL_STATE_BYTES,
                        )
                        .unwrap();
                    assert_eq!(Some(bytes.len() as u64), entry.size);
                    let value: Value = serde_json::from_slice(&bytes).unwrap();
                    let crypt = value.get("os_crypt");
                    json!({
                        "fileId":entry.id, "path":entry.path, "size":entry.size,
                        "osCryptPresent":crypt.is_some(), "osCryptType":value_type(crypt),
                        "encrypted_key":field_metadata(crypt.and_then(|value| value.get("encrypted_key"))),
                        "app_bound_encrypted_key":field_metadata(crypt.and_then(|value| value.get("app_bound_encrypted_key")))
                    })
                })
                .collect::<Vec<_>>();
            Ok(reports)
        })
        .unwrap();
    let path = output.join("chromium-key-metadata.json");
    super::write_report(
        &path,
        &json!({"caseId":active.meta.id.0, "sourceId":source.id.0, "localStates":reports}),
    );
    eprintln!("CHROMIUM_METADATA_REPORT {}", path.display());
    assert!(!reports.is_empty(), "no Local State catalog entry found");
}

fn local_state_entries(sql: &rusqlite::Connection, source_id: &str) -> Vec<FileEntryRecord> {
    let store = persistence_source::detail::require_source_store(sql).unwrap();
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = catalog
            .list_all(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .unwrap();
        for record in page.records {
            assert_eq!(record.data_source_id, source_id);
            if !record.deleted
                && record.entry_type == CatalogEntryType::File
                && record.name.eq_ignore_ascii_case("Local State")
            {
                assert!(records.len() < 64, "too many Local State entries");
                records.push(record);
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    records.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.id.cmp(&right.id))
    });
    records
}

fn field_metadata(value: Option<&Value>) -> Value {
    let encoded = value.and_then(Value::as_str);
    let decoded = encoded.map(|value| STANDARD.decode(value));
    json!({
        "present":value.is_some(), "valueType":value_type(value),
        "base64Length":encoded.map(str::len),
        "base64Valid":decoded.as_ref().map(Result::is_ok),
        "decodedLength":decoded.as_ref().and_then(|result| result.as_ref().ok()).map(Vec::len)
    })
}

fn value_type(value: Option<&Value>) -> &'static str {
    match value {
        None => "missing",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}
