use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink, ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_REPORTED_ENTRIES: usize = 100;

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
        let lower = file_path.replace('\\', "/").to_ascii_lowercase();
        let name = lower.rsplit('/').next().unwrap_or(&lower);
        name.starts_with("thumbcache_") && name.ends_with(".db") && name != "thumbcache_idx.db"
    }

    fn run(
        &self,
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut bytes = Vec::new();
        ctx.reader
            .by_ref()
            .take(MAX_CACHE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_CACHE_BYTES {
            return Err("Thumbnail cache exceeds the supported input limit".into());
        }
        let cache = imagecache::thumbcache::read(&bytes);
        let version = cache.version.ok_or_else(|| cache.problems.join("; "))?;
        if !cache.problems.is_empty() {
            return Err(cache.problems.join("; "));
        }
        let errors = cache
            .entries
            .iter()
            .filter(|entry| !entry.header_checksum_valid || !entry.data_checksum_valid)
            .map(|entry| {
                format!(
                    "Thumbnail cache entry at {} failed checksum verification",
                    entry.offset
                )
            })
            .collect();
        let description = cache
            .size_name()
            .ok_or("Unsupported thumbnail cache type")?;
        let entries = cache
            .entries
            .iter()
            .take(MAX_REPORTED_ENTRIES)
            .map(|entry| {
                serde_json::json!({
                    "hash": format!("{:016X}", entry.hash),
                    "data_size": entry.data.len(),
                    "identifier": entry.identifier,
                    "data_offset": entry.data_offset,
                    "header_checksum_verified": entry.header_checksum_valid,
                    "data_checksum_verified": entry.data_checksum_valid,
                })
            })
            .collect::<Vec<_>>();
        let total_data_size = cache
            .entries
            .iter()
            .map(|entry| entry.data.len() as u64)
            .sum::<u64>();
        let attrs = BTreeMap::from([
            ("file_size".into(), (bytes.len() as u64).into()),
            ("version".into(), version.number().into()),
            ("cache_type".into(), cache.cache_type.into()),
            ("cache_type_desc".into(), description.into()),
            ("entry_count".into(), cache.entries.len().into()),
            ("total_thumbnail_data_size".into(), total_data_size.into()),
            ("entries".into(), entries.into()),
            (
                "entries_truncated".into(),
                (cache.entries.len() > MAX_REPORTED_ENTRIES).into(),
            ),
        ]);
        sink.write_artifact(new_artifact(
            "Thumbcache",
            format!("Thumbcache: {}", ctx.file_path),
            format!(
                "Windows Explorer thumbnail cache ({description}), {} entries, {} bytes",
                cache.entries.len(),
                bytes.len()
            ),
            Some(&ctx.file_id),
            attrs,
        ));
        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events: 0,
            errors,
        })
    }
}
