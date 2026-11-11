use super::ArtifactRepo;
use crate::DbResult;
use domain::Artifact;

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
        _case_id: &str,
        source_id: &str,
    ) -> DbResult<()> {
        if !crate::source_fjall::upsert_artifacts(self.conn, artifacts, source_id)? {
            return Err(crate::DbError::System(
                "source detail storage is missing".into(),
            ));
        }
        Ok(())
    }
}
