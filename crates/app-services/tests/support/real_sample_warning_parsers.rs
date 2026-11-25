use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use persistence_source::fjall::FileEntryRecord;
use serde_json::{json, Value};
use std::io::Cursor;

pub(super) fn observe(entry: &FileEntryRecord, bytes: &[u8]) -> Value {
    if Some(bytes.len() as u64) != entry.size {
        return json!({"status": "inputPrefixOnly"});
    }
    let path = entry.path.to_ascii_lowercase();
    if path.ends_with(".automaticdestinations-ms") {
        return automatic_destinations(entry, bytes);
    }
    let extractor: Box<dyn ArtifactExtractor> = if path.ends_with(".pf") {
        Box::new(artifacts_windows::PrefetchExtractor)
    } else if path.ends_with(".lnk") {
        Box::new(artifacts_windows::LnkExtractor)
    } else if entry.name.to_ascii_lowercase().starts_with("thumbcache") {
        Box::new(artifacts_windows::ThumbcacheExtractor)
    } else {
        return Value::Null;
    };
    let mut sink = VecSink::new();
    let result = extractor.run(
        ArtifactContext {
            file_id: domain::FileEntryId(entry.id.clone()),
            file_path: entry.path.clone(),
            reader: Box::new(Cursor::new(bytes.to_vec())),
        },
        &mut sink,
    );
    match result {
        Ok(report) => json!({
            "extractor": extractor.id(),
            "artifacts": report.artifacts_found,
            "timelineEvents": report.timeline_events,
            "warnings": report.errors,
        }),
        Err(error) => json!({"extractor": extractor.id(), "error": error}),
    }
}

fn automatic_destinations(entry: &FileEntryRecord, bytes: &[u8]) -> Value {
    let file = match shell::compound::CompoundFile::parse(bytes) {
        Ok(file) => file,
        Err(error) => return json!({"compoundError": error.to_string()}),
    };
    let stream = match file.stream("DestList") {
        Ok(Some(stream)) => json!({
            "state": "present",
            "bytes": stream.len(),
            "headerHex": hex::encode(&stream[..stream.len().min(32)]),
        }),
        Ok(None) => json!({"state": "missing"}),
        Err(error) => json!({"state": "error", "reason": error.to_string()}),
    };
    let parsed = shell::jumplist::automatic(bytes).unwrap();
    let mut sink = VecSink::new();
    let production = artifacts_windows::JumpListExtractor.run(
        ArtifactContext {
            file_id: domain::FileEntryId(entry.id.clone()),
            file_path: entry.path.clone(),
            reader: Box::new(Cursor::new(bytes.to_vec())),
        },
        &mut sink,
    );
    let production = match production {
        Ok(report) => json!({
            "artifacts": report.artifacts_found,
            "timelineEvents": report.timeline_events,
            "warnings": report.errors,
        }),
        Err(error) => json!({"error": error}),
    };
    json!({
        "compoundProblems": file.problems,
        "streamNames": file.entries.iter().map(|entry| &entry.name).collect::<Vec<_>>(),
        "destList": stream,
        "version": parsed.version,
        "destinations": parsed.destinations.len(),
        "entries": parsed.destinations.iter().map(|entry| json!({
            "entry": entry.entry,
            "path": entry.path,
            "lastModifiedFiletime": entry.last_used,
            "interactionCount": entry.access_count,
            "hostname": entry.hostname,
            "hasLink": entry.link.is_some(),
        })).collect::<Vec<_>>(),
        "problems": parsed.problems,
        "production": production,
    })
}
