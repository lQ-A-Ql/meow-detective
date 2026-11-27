use super::*;
use crate::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};
use domain::structure::StorageObjectVersion;

impl StorageObjectRepo<'_> {
    pub fn bind_version(&self, case_id: &str, version: &StorageObjectVersion) -> DbResult<()> {
        let transaction = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()?;
        let object = self
            .find(&version.storage_object_id)?
            .ok_or_else(|| DbError::System("storage object missing".into()))?;
        let source_case = DataSourceRepo::new(self.conn)
            .case_id(&domain::DataSourceId(version.derived_source_id.clone()))?;
        if object.case_id != case_id
            || source_case.0 != case_id
            || version.id.is_empty()
            || version.adapter_version.is_empty()
            || version.native_versions.is_empty()
            || version.mapping_sha256.len() != 64
            || !version
                .mapping_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(DbError::System("invalid storage object version".into()));
        }
        SourceRevisionRepo::new(self.conn)
            .require_current_inputs(case_id, &version.derived_source_id)?;
        if let Some(prior) = self.version_for_source(case_id, &version.derived_source_id)? {
            return if prior == *version {
                Ok(())
            } else {
                Err(DbError::System("storage object version changed".into()))
            };
        }
        let native = serde_json::to_string(&version.native_versions)
            .map_err(|error| DbError::System(error.to_string()))?;
        self.conn.execute("INSERT INTO storage_object_versions
            (id, storage_object_id, derived_source_id, adapter_version, mapping_sha256, native_versions_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![version.id, version.storage_object_id,
            version.derived_source_id, version.adapter_version, version.mapping_sha256, native])?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn version_for_source(
        &self,
        case_id: &str,
        source_id: &str,
    ) -> DbResult<Option<StorageObjectVersion>> {
        let source_case =
            DataSourceRepo::new(self.conn).case_id(&domain::DataSourceId(source_id.into()))?;
        if source_case.0 != case_id {
            return Err(DbError::System("storage version case mismatch".into()));
        }
        let record: Option<(String, String, String, String, String)> = self.conn.query_row(
            "SELECT version.id, storage_object_id, adapter_version, mapping_sha256, native_versions_json
             FROM storage_object_versions AS version JOIN storage_objects AS object ON object.id=version.storage_object_id
             WHERE derived_source_id=?1 AND object.case_id=?2", params![source_id, case_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        ).optional()?;
        record
            .map(
                |(id, storage_object_id, adapter_version, mapping_sha256, native)| {
                    Ok(StorageObjectVersion {
                        id,
                        storage_object_id,
                        derived_source_id: source_id.into(),
                        adapter_version,
                        mapping_sha256,
                        native_versions: serde_json::from_str(&native)
                            .map_err(|error| DbError::System(error.to_string()))?,
                    })
                },
            )
            .transpose()
    }
}
