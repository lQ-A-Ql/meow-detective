use super::ArtifactRepo;
use crate::{repositories::fingerprint_repo::ForensicFingerprintRepo, DbError, DbResult};
use domain::Artifact;
use rusqlite::params;

impl ArtifactRepo<'_> {
    pub fn insert_batch(
        &self,
        artifacts: &[Artifact],
        case_id: &str,
        source_id: &str,
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        ArtifactRepo::new(&tx).insert_batch_in_transaction(artifacts, case_id, source_id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_batch_in_transaction(
        &self,
        artifacts: &[Artifact],
        case_id: &str,
        source_id: &str,
    ) -> DbResult<()> {
        if crate::source_fjall::upsert_artifacts(self.conn, artifacts, source_id)? {
            let fingerprints = ForensicFingerprintRepo::new(self.conn);
            for artifact in artifacts {
                fingerprints.upsert_in_transaction(&domain::ForensicFingerprint::for_artifact(
                    artifact, case_id, source_id,
                ))?;
            }
            return Ok(());
        }
        let mut statement = self.conn.prepare_cached(
            "INSERT INTO artifacts (id,case_id,data_source_id,artifact_type,source_object_id,
             extractor_id,extractor_version,confidence,source_attribution,title,summary,attrs,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
        )?;
        let fingerprints = ForensicFingerprintRepo::new(self.conn);
        let available = fingerprints.is_available()?;
        for artifact in artifacts {
            let attrs = serde_json::to_string(&artifact.attrs)
                .map_err(|error| DbError::System(error.to_string()))?;
            statement.execute(params![
                artifact.id.0,
                case_id,
                source_id,
                artifact.family,
                artifact.source_object_id.as_ref().map(|id| &id.0),
                artifact.extractor_id,
                artifact.extractor_version,
                artifact.confidence,
                artifact.source_attribution,
                artifact.title,
                artifact.summary,
                attrs,
                artifact.created_at.to_rfc3339(),
            ])?;
            if available {
                fingerprints.upsert_in_transaction(&domain::ForensicFingerprint::for_artifact(
                    artifact, case_id, source_id,
                ))?;
            }
        }
        Ok(())
    }
}
