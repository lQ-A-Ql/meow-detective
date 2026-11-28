use std::collections::BTreeMap;

use domain::Artifact;
use serde_json::{json, Value};

use super::model::{normalized_time, Evidence, FactProjection};
use crate::structure_analysis::StructureAnalysisError;

#[derive(Default)]
pub(super) struct JournalIdentities(BTreeMap<(String, String, String), JournalIdentity>);

struct JournalIdentity {
    first: Evidence,
    last: Evidence,
    count: u64,
}

impl JournalIdentities {
    pub fn observe(
        &mut self,
        artifact: &Artifact,
        source: &Evidence,
    ) -> persistence_sqlite::DbResult<()> {
        if artifact.extractor_id.as_deref() != Some("linux.journal") {
            return Ok(());
        }
        let text = |key: &str| {
            artifact
                .attrs
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let identity = (text("machineId"), text("bootId"), text("hostname"));
        if identity.0.is_empty() && identity.1.is_empty() {
            return Ok(());
        }
        if !self.0.contains_key(&identity) && self.0.len() >= 256 {
            return Err(persistence_sqlite::DbError::System(
                "journal identity bound exceeded".into(),
            ));
        }
        let mut evidence = source.clone();
        evidence.artifact_id = Some(artifact.id.0.clone());
        evidence.file_id = artifact.source_object_id.as_ref().map(|id| id.0.clone());
        evidence.parser = "linux.journal".into();
        evidence.parser_version = artifact.extractor_version.clone().unwrap_or_default();
        evidence.locator = artifact.source_attribution.clone();
        evidence.assertion_kind = "historical_observation".into();
        evidence.observed_at =
            normalized_time(artifact.attrs.get("timestamp").and_then(Value::as_str))
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        let entry = self.0.entry(identity).or_insert_with(|| JournalIdentity {
            first: evidence.clone(),
            last: evidence.clone(),
            count: 0,
        });
        entry.count += 1;
        if evidence.observed_at < entry.first.observed_at {
            entry.first = evidence.clone();
        }
        if evidence.observed_at > entry.last.observed_at {
            entry.last = evidence;
        }
        Ok(())
    }

    pub fn project(
        self,
        source: &Evidence,
        graph: &mut FactProjection,
    ) -> Result<(), StructureAnalysisError> {
        for ((machine, boot, hostname), identity) in self.0 {
            let os = graph.entity(
                &source.acquisition_id,
                "os_instance",
                &source.source_revision.source_id,
                "Linux system",
            );
            graph.fact(&os, None, "os_observation", &identity.first, json!({
                "machineId": machine,
                "bootId": boot,
                "hostname": hostname,
                "firstObservedAt": identity.first.observed_at,
                "lastObservedAt": identity.last.observed_at,
                "lastEvidence": identity.last,
                "observationCount": identity.count,
                "evidenceQuery": {"family": "LinuxJournal", "machineId": machine, "bootId": boot, "hostname": hostname},
            }))?;
        }
        Ok(())
    }
}
