//! Windows Jump List artifact parser.
//!
//! Parses AutomaticDestinations (.ms-* files) and CustomDestinations
//! (.customDestinations-ms) Jump List files.
//!
//! AutomaticDestinations are OLE compound documents containing embedded
//! LNK files. This parser extracts the LNK entries and delegates to
//! the LnkExtractor for detailed parsing.

use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

/// Jump List parser for AutomaticDestinations files.
pub struct JumpListExtractor;

impl JumpListExtractor {
    /// Extract LNK data blocks from OLE compound document.
    ///
    /// This is a simplified parser that looks for LNK signatures within the data.
    /// A full OLE parser would be more robust, but this handles common cases.
    fn extract_lnk_from_ole(data: &[u8]) -> Vec<Vec<u8>> {
        let mut lnk_blocks = Vec::new();

        // Search for LNK signatures in the data
        let mut i = 0;
        // Shell Link CLSID first 4 bytes: 0x00021401
        while i + 4 < data.len() {
            // Look for the Shell Link CLSID pattern
            if data[i] == 0x01 && data[i + 1] == 0x14 && data[i + 2] == 0x02 && data[i + 3] == 0x00 {
                // Check if this looks like a valid LNK header
                if i + 76 <= data.len() {
                    let header_size = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
                    if header_size == 0x4C {
                        // Found a potential LNK block
                        // Read the link flags to determine total size
                        let flags = if i + 20 <= data.len() {
                            u32::from_le_bytes([data[i + 20], data[i + 21], data[i + 22], data[i + 23]])
                        } else {
                            0
                        };

                        // Estimate block size (header + optional sections)
                        let mut block_size = 76; // Minimum header size

                        // Add LinkTargetIDList size if present
                        if flags & 0x00000001 != 0 && i + block_size + 2 <= data.len() {
                            let id_list_size = u16::from_le_bytes([data[i + block_size], data[i + block_size + 1]]) as usize;
                            block_size += 2 + id_list_size;
                        }

                        // Add LinkInfo size if present
                        if flags & 0x00000002 != 0 && i + block_size + 4 <= data.len() {
                            let link_info_size = u32::from_le_bytes([
                                data[i + block_size],
                                data[i + block_size + 1],
                                data[i + block_size + 2],
                                data[i + block_size + 3],
                            ]) as usize;
                            block_size += link_info_size;
                        }

                        // Ensure we don't exceed data bounds
                        if i + block_size <= data.len() {
                            lnk_blocks.push(data[i..i + block_size].to_vec());
                        }
                    }
                }
            }
            i += 1;
        }

        lnk_blocks
    }
}

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
            description: Some("Windows Jump List entries (.ms-* and .customDestinations-ms)".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        let lower = file_path.to_lowercase();
        // AutomaticDestinations: {AppID}.ms-* files
        // CustomDestinations: *.customDestinations-ms files
        // Also support direct .ms- files (e.g., 5f7b5f7e3243a7b8.ms-abc)
        lower.ends_with(".customdestinations-ms")
            || (lower.contains(".ms-") && !lower.contains(".msg"))
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut reader = ctx.reader;
        let mut data = Vec::new();
        reader.read_to_end(&mut data).map_err(|e| e.to_string())?;

        if data.len() < 4 {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec![],
            });
        }

        // Try to extract LNK blocks from the data
        let lnk_blocks = Self::extract_lnk_from_ole(&data);

        if lnk_blocks.is_empty() {
            // No LNK blocks found, create a generic JumpList artifact
            let mut attrs = BTreeMap::new();
            attrs.insert("file_size".into(), (data.len() as u64).into());
            attrs.insert("format".into(), "AutomaticDestinations".into());

            let artifact = new_artifact(
                "JumpList",
                format!("JumpList: {}", ctx.file_path),
                format!("Jump List file, {} bytes", data.len()),
                Some(&ctx.file_id),
                attrs,
            );
            sink.write_artifact(artifact);

            return Ok(ExtractorReport {
                artifacts_found: 1,
                timeline_events: 0,
                errors: vec![],
            });
        }

        // Parse each embedded LNK block
        let mut total_artifacts = 0u32;
        let mut total_events = 0u32;

        for (index, lnk_data) in lnk_blocks.iter().enumerate() {
            // Create a sub-context for the LNK extractor
            let lnk_ctx = ArtifactContext {
                file_id: domain::FileEntryId(format!("{}:lnk:{}", ctx.file_id.0, index)),
                file_path: format!("{}:lnk[{}]", ctx.file_path, index),
                reader: Box::new(std::io::Cursor::new(lnk_data.clone())),
            };

            // Use the LNK extractor
            let lnk_extractor = crate::lnk::parser::LnkExtractor;
            match lnk_extractor.run(lnk_ctx, sink) {
                Ok(report) => {
                    total_artifacts += report.artifacts_found;
                    total_events += report.timeline_events;
                }
                Err(e) => {
                    // Log error but continue with other LNK blocks
                    tracing::warn!("Failed to parse LNK block {}: {}", index, e);
                }
            }
        }

        // Create a summary artifact for the JumpList itself
        let mut attrs = BTreeMap::new();
        attrs.insert("file_size".into(), (data.len() as u64).into());
        attrs.insert("lnk_count".into(), (lnk_blocks.len() as u64).into());
        attrs.insert("format".into(), "AutomaticDestinations".into());

        let artifact = new_artifact(
            "JumpList",
            format!("JumpList: {}", ctx.file_path),
            format!(
                "Jump List with {} embedded shortcuts",
                lnk_blocks.len()
            ),
            Some(&ctx.file_id),
            attrs,
        );
        sink.write_artifact(artifact);
        total_artifacts += 1;

        Ok(ExtractorReport {
            artifacts_found: total_artifacts,
            timeline_events: total_events,
            errors: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artifacts_core::VecSink;

    #[test]
    fn jump_list_supports_path() {
        let extractor = JumpListExtractor;
        assert!(extractor.supports_path("C:/Users/test/AppData/Roaming/Microsoft/Windows/Recent/5f7b5f7e3243a7b8.ms-abc"));
        assert!(extractor.supports_path("C:/Users/test/AppData/Roaming/Microsoft/Windows/Recent/Custom/custom.customDestinations-ms"));
        assert!(!extractor.supports_path("C:/Users/test/file.txt"));
        assert!(!extractor.supports_path("C:/Users/test/file.lnk"));
    }

    #[test]
    fn jump_list_truncated_no_panic() {
        let extractor = JumpListExtractor;
        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("test".to_string()),
            file_path: "test.ms-abc".into(),
            reader: Box::new(std::io::Cursor::new(vec![0u8; 10])),
        };
        let mut sink = VecSink::new();
        let _ = extractor.run(ctx, &mut sink);
    }
}
