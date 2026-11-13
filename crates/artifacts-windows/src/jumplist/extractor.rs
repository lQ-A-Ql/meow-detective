use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink, ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

const MAX_JUMPLIST_BYTES: u64 = 64 * 1024 * 1024;

pub struct JumpListExtractor;

impl ArtifactExtractor for JumpListExtractor {
    fn id(&self) -> &'static str {
        "jumplist"
    }

    fn display_name(&self) -> &'static str {
        "Windows Jump List Parser"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "JumpList".into(),
            description: Some("Windows AutomaticDestinations and CustomDestinations".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        let lower = file_path.to_ascii_lowercase();
        lower.ends_with(".automaticdestinations-ms") || lower.ends_with(".customdestinations-ms")
    }

    fn run(
        &self,
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut data = Vec::new();
        ctx.reader
            .by_ref()
            .take(MAX_JUMPLIST_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|error| error.to_string())?;
        if data.len() as u64 > MAX_JUMPLIST_BYTES {
            return Err("Jump List exceeds the supported input limit".into());
        }
        if ctx
            .file_path
            .to_ascii_lowercase()
            .ends_with(".automaticdestinations-ms")
        {
            extract_automatic(&ctx, &data, sink)
        } else if ctx
            .file_path
            .to_ascii_lowercase()
            .ends_with(".customdestinations-ms")
        {
            extract_custom(&ctx, &data, sink)
        } else {
            Err("Unsupported Jump List source path".into())
        }
    }
}

fn extract_automatic(
    ctx: &ArtifactContext,
    bytes: &[u8],
    sink: &mut dyn ArtifactSink,
) -> Result<ExtractorReport, String> {
    let list = shell::jumplist::automatic(bytes).map_err(|error| error.to_string())?;
    if !matches!(list.version, 1 | 3 | 4) {
        return Err("AutomaticDestinations has no supported DestList header".into());
    }
    let mut report = ExtractorReport {
        artifacts_found: 0,
        timeline_events: 0,
        errors: list.problems,
    };
    let mut metadata = Vec::new();
    for destination in list.destinations {
        let entry_name = format!("{:x}", destination.entry);
        if let Some(link) = &destination.link {
            append_link(ctx, link, &entry_name, sink, &mut report);
        }
        metadata.push(serde_json::json!({
            "entry": entry_name,
            "path": destination.path,
            "lastUsedFiletime": destination.last_used,
            "accessCount": destination.access_count,
            "pin": destination.pin,
            "hostname": destination.hostname,
        }));
    }
    write_summary(
        ctx,
        sink,
        bytes.len(),
        "AutomaticDestinations",
        report.artifacts_found,
        metadata,
    );
    report.artifacts_found += 1;
    Ok(report)
}

fn extract_custom(
    ctx: &ArtifactContext,
    bytes: &[u8],
    sink: &mut dyn ArtifactSink,
) -> Result<ExtractorReport, String> {
    let list = shell::jumplist::custom(bytes);
    if list.categories.is_empty() {
        return Err("CustomDestinations has no readable category".into());
    }
    let mut report = ExtractorReport {
        artifacts_found: 0,
        timeline_events: 0,
        errors: list.problems,
    };
    let mut metadata = Vec::new();
    for (category_index, category) in list.categories.into_iter().enumerate() {
        let name = match category.kind {
            shell::jumplist::Kind::Custom(name) => name,
            shell::jumplist::Kind::Known(id) => format!("Known({id})"),
            shell::jumplist::Kind::Tasks => "Tasks".into(),
        };
        metadata.push(serde_json::json!({"category": name, "linkCount": category.links.len()}));
        for (link_index, link) in category.links.iter().enumerate() {
            append_link(
                ctx,
                link,
                &format!("category:{category_index}/link:{link_index}"),
                sink,
                &mut report,
            );
        }
    }
    write_summary(
        ctx,
        sink,
        bytes.len(),
        "CustomDestinations",
        report.artifacts_found,
        metadata,
    );
    report.artifacts_found += 1;
    Ok(report)
}

fn append_link(
    ctx: &ArtifactContext,
    link: &shell::lnk::Link,
    entry: &str,
    sink: &mut dyn ArtifactSink,
    report: &mut ExtractorReport,
) {
    let decoded = crate::lnk::decode::project_link(link);
    let child = crate::lnk::parser::write_decoded_link(ctx, decoded, Some(entry), sink);
    report.artifacts_found += child.artifacts_found;
    report.timeline_events += child.timeline_events;
    report.errors.extend(child.errors);
}

fn write_summary(
    ctx: &ArtifactContext,
    sink: &mut dyn ArtifactSink,
    file_size: usize,
    format: &str,
    lnk_count: u32,
    entries: Vec<serde_json::Value>,
) {
    let attrs = BTreeMap::from([
        ("file_size".into(), (file_size as u64).into()),
        ("format".into(), format.into()),
        ("lnk_count".into(), lnk_count.into()),
        ("entries".into(), entries.into()),
    ]);
    sink.write_artifact(new_artifact(
        "JumpList",
        format!("JumpList: {}", ctx.file_path),
        format!("{format} with {lnk_count} embedded shortcuts"),
        Some(&ctx.file_id),
        attrs,
    ));
}
