use super::entries::{enumerate, EntrySummary};
use super::header::ThumbcacheHeader;
use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink, ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

pub struct ThumbcacheExtractor;

impl ArtifactExtractor for ThumbcacheExtractor {
    fn id(&self) -> &'static str {
        "thumbcache"
    }

    fn display_name(&self) -> &'static str {
        "Windows Thumbcache Parser"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "Thumbcache".into(),
            description: Some("Windows Explorer thumbnail cache (thumbcache_*.db)".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        let lower = file_path.to_lowercase();
        lower.contains("thumbcache") && lower.ends_with(".db")
    }

    fn run(
        &self,
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut data = Vec::new();
        ctx.reader
            .read_to_end(&mut data)
            .map_err(|error| error.to_string())?;
        let header = match ThumbcacheHeader::parse(&data) {
            Ok(header) => header,
            Err(error) => return Ok(failure_report(error)),
        };
        let entries = enumerate(&data, header.entry_offset);
        sink.write_artifact(build_artifact(&ctx, data.len(), &header, &entries));
        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events: 0,
            errors: Vec::new(),
        })
    }
}

fn build_artifact(
    ctx: &ArtifactContext,
    file_size: usize,
    header: &ThumbcacheHeader,
    entries: &EntrySummary,
) -> domain::Artifact {
    let description = header.cache_type_description();
    let mut attrs = BTreeMap::new();
    attrs.insert("file_size".into(), (file_size as u64).into());
    attrs.insert("header_size".into(), header.header_size.into());
    attrs.insert("version".into(), header.version.into());
    attrs.insert("cache_type".into(), header.cache_type.into());
    attrs.insert("cache_type_desc".into(), description.into());
    if entries.count > 0 {
        attrs.insert("entry_count".into(), entries.count.into());
        attrs.insert(
            "total_thumbnail_data_size".into(),
            entries.total_data_size.into(),
        );
        attrs.insert(
            "entries".into(),
            serde_json::Value::Array(entries.entries.clone()),
        );
    }
    new_artifact(
        "Thumbcache",
        format!("Thumbcache: {}", ctx.file_path),
        summary(file_size, description, entries),
        Some(&ctx.file_id),
        attrs,
    )
}

fn summary(file_size: usize, description: &str, entries: &EntrySummary) -> String {
    if entries.count == 0 {
        format!("Windows Explorer thumbnail cache ({description}), {file_size} bytes")
    } else {
        format!(
            "Windows Explorer thumbnail cache ({}), {} entries, {} bytes ({} bytes thumbnail data)",
            description, entries.count, file_size, entries.total_data_size
        )
    }
}

fn failure_report(error: String) -> ExtractorReport {
    ExtractorReport {
        artifacts_found: 0,
        timeline_events: 0,
        errors: vec![error],
    }
}
