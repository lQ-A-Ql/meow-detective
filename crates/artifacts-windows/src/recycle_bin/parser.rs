//! Windows Recycle Bin metadata extraction through the read-only trash-core reader.

use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

const MAX_INDEX_BYTES: u64 = 128 * 1024;

pub struct RecycleBinExtractor;

impl ArtifactExtractor for RecycleBinExtractor {
    fn id(&self) -> &'static str {
        "recycle_bin"
    }

    fn display_name(&self) -> &'static str {
        "Windows Recycle Bin Parser"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "RecycleBin".into(),
            description: Some("Windows Recycle Bin metadata ($I files)".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        let path = file_path.replace('\\', "/").to_ascii_lowercase();
        path.split('/').any(|part| part == "$recycle.bin")
            && path
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("$i"))
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut bytes = Vec::new();
        ctx.reader
            .take(MAX_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_INDEX_BYTES {
            return Err("Recycle Bin index exceeds the supported input limit".into());
        }
        let parsed = trash_core::parse_index(&bytes).map_err(|error| error.to_string())?;
        let mut attrs = BTreeMap::from([
            ("recovered_file_size".into(), parsed.original_size.into()),
            ("original_path".into(), parsed.original_path.clone().into()),
            (
                "format_version".into(),
                match parsed.version {
                    trash_core::IndexVersion::V1 => 1u32.into(),
                    trash_core::IndexVersion::V2 => 2u32.into(),
                },
            ),
        ]);
        if let Some(timestamp) = parsed.deleted_at {
            attrs.insert("deletion_time".into(), timestamp.to_rfc3339().into());
        }
        sink.write_artifact(new_artifact(
            "RecycleBin",
            "Recycle Bin: deleted file".into(),
            format!("Deleted: {}", parsed.original_path),
            Some(&ctx.file_id),
            attrs,
        ));
        let timeline_events = if let Some(timestamp) = parsed.deleted_at {
            sink.write_timeline_event(new_timeline_event(
                &ctx.file_id,
                "FILE_DELETED",
                timestamp,
                "File deleted".into(),
                format!("File deleted at {}", timestamp.to_rfc3339()),
                BTreeMap::from([("path".into(), parsed.original_path.into())]),
            ));
            1
        } else {
            0
        };
        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events,
            errors: Vec::new(),
        })
    }
}
