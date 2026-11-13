use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink, ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

const MAX_SRU_BYTES: u64 = 64 * 1024 * 1024;

pub struct SruExtractor;

impl ArtifactExtractor for SruExtractor {
    fn id(&self) -> &'static str {
        "sru"
    }

    fn display_name(&self) -> &'static str {
        "Windows SRU Database Metadata"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "SRU".into(),
            description: Some("Windows System Resource Usage database metadata".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        file_path
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("SRUDB.dat"))
    }

    fn run(
        &self,
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut bytes = Vec::new();
        ctx.reader
            .by_ref()
            .take(MAX_SRU_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_SRU_BYTES {
            return Err("SRU database exceeds the supported input limit".into());
        }
        let database = ese::Database::open(&bytes).map_err(|error| error.to_string())?;
        if !matches!(database.header.format_version, 0x620 | 0x623) {
            return Err("Unsupported ESE database format version".into());
        }
        let (state, state_name) = database_state(database.header.state);
        let attrs = BTreeMap::from([
            ("file_size".into(), (bytes.len() as u64).into()),
            ("format".into(), "ESE/Jet Blue".into()),
            ("database_type".into(), "System Resource Usage".into()),
            ("metadata_only".into(), true.into()),
            (
                "ese_db_version".into(),
                database.header.format_version.into(),
            ),
            (
                "ese_format_revision".into(),
                database.header.format_revision.into(),
            ),
            ("page_size".into(), database.header.page_size.into()),
            (
                "estimated_pages".into(),
                (bytes.len() as u64 / u64::from(database.header.page_size)).into(),
            ),
            ("database_state".into(), state.into()),
            ("database_state_desc".into(), state_name.into()),
            (
                "header_from_shadow".into(),
                database.header.from_shadow.into(),
            ),
        ]);
        sink.write_artifact(new_artifact(
            "SRU",
            format!("SRU Database: {}", ctx.file_path),
            format!(
                "Windows System Resource Usage database metadata, {} bytes",
                bytes.len()
            ),
            Some(&ctx.file_id),
            attrs,
        ));
        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events: 0,
            errors: database.problems,
        })
    }
}

fn database_state(state: ese::DatabaseState) -> (u32, &'static str) {
    match state {
        ese::DatabaseState::JustCreated => (1, "JustCreated"),
        ese::DatabaseState::DirtyShutdown => (2, "DirtyShutdown"),
        ese::DatabaseState::CleanShutdown => (3, "CleanShutdown"),
        ese::DatabaseState::BeingConverted => (4, "BeingConverted"),
        ese::DatabaseState::ForceDetach => (5, "ForceDetach"),
        ese::DatabaseState::Other(value) => (value, "Unknown"),
    }
}
