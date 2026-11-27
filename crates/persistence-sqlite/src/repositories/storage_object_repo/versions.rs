use super::*;
use crate::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};
use domain::structure::{
    AcquisitionId, NativeObjectId, ObjectVersionId, RevisionConsistency, SourceRevisionRef,
    StorageObjectVersion, StorageObjectVersionKind,
};

impl StorageObjectRepo<'_> {
    pub fn bind_version(&self, case_id: &str, version: &StorageObjectVersion) -> DbResult<()> {
        version
            .validate()
            .map_err(|error| DbError::System(error.to_string()))?;
        let transaction = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()?;
        let object = self
            .find(&version.storage_object_id)?
            .ok_or_else(|| DbError::System("storage object missing".into()))?;
        if object.case_id != case_id {
            return Err(DbError::System("storage object case mismatch".into()));
        }
        require_source_case(self.conn, case_id, &version.owner_source_id)?;
        require_source_case(self.conn, case_id, &version.derived_source_id)?;
        let revisions = SourceRevisionRepo::new(self.conn);
        for parent in &version.parent_source_revisions {
            if parent.case_id != case_id {
                return Err(DbError::System(
                    "storage object parent revision belongs to another case".into(),
                ));
            }
            revisions.require_current(parent)?;
        }
        if version.version_kind == StorageObjectVersionKind::DerivedOutput {
            revisions.require_current_inputs(case_id, &version.derived_source_id)?;
        }
        let native_object_json = serde_json::to_string(&version.native_object_id)
            .map_err(|error| DbError::System(error.to_string()))?;
        let acquisition_json = serde_json::to_string(&version.acquisition_ids)
            .map_err(|error| DbError::System(error.to_string()))?;
        let parent_json = serde_json::to_string(&version.parent_source_revisions)
            .map_err(|error| DbError::System(error.to_string()))?;
        let native_json = serde_json::to_string(&version.native_versions)
            .map_err(|error| DbError::System(error.to_string()))?;
        if let Some(prior) = self.version_by_id(case_id, &version.id.0)? {
            return if prior == *version {
                if let Some(transaction) = transaction {
                    transaction.commit()?;
                }
                Ok(())
            } else {
                Err(DbError::System("storage object version changed".into()))
            };
        }
        self.conn.execute(
            "INSERT INTO storage_object_versions (
                id, storage_object_id, native_object_id_json, owner_source_id,
                derived_source_id, adapter_version, policy_version, mapping_sha256,
                acquisition_ids_json, parent_source_revisions_json,
                native_versions_json, consistency, version_kind
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                version.id.0,
                version.storage_object_id,
                native_object_json,
                version.owner_source_id,
                version.derived_source_id,
                version.adapter_version,
                version.policy_version,
                version.mapping_sha256,
                acquisition_json,
                parent_json,
                native_json,
                version.consistency.as_str(),
                version.version_kind.as_str(),
            ],
        )?;
        self.reconcile_identity(case_id, version, &native_json)?;
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
        Ok(self
            .versions_for_source(case_id, source_id)?
            .into_iter()
            .find(|version| version.version_kind == StorageObjectVersionKind::DerivedOutput))
    }

    pub fn versions_for_source(
        &self,
        case_id: &str,
        source_id: &str,
    ) -> DbResult<Vec<StorageObjectVersion>> {
        require_source_case(self.conn, case_id, source_id)?;
        self.query_versions(
            "WHERE version.owner_source_id=?1 AND object.case_id=?2",
            params![source_id, case_id],
        )
    }

    pub fn versions_for_object(
        &self,
        case_id: &str,
        storage_object_id: &str,
    ) -> DbResult<Vec<StorageObjectVersion>> {
        self.query_versions(
            "WHERE version.storage_object_id=?1 AND object.case_id=?2",
            params![storage_object_id, case_id],
        )
    }

    pub fn version_by_id(
        &self,
        case_id: &str,
        version_id: &str,
    ) -> DbResult<Option<StorageObjectVersion>> {
        self.query_versions(
            "WHERE version.id=?1 AND object.case_id=?2",
            params![version_id, case_id],
        )
        .map(|mut versions| versions.pop())
    }

    fn query_versions(
        &self,
        filter: &str,
        params: impl rusqlite::Params,
    ) -> DbResult<Vec<StorageObjectVersion>> {
        let sql = format!(
            "SELECT version.id, version.storage_object_id, version.native_object_id_json,
                    version.owner_source_id, version.derived_source_id,
                    version.adapter_version, version.policy_version,
                    version.mapping_sha256, version.acquisition_ids_json,
                    version.parent_source_revisions_json, version.native_versions_json,
                    version.consistency, version.version_kind
             FROM storage_object_versions AS version
             JOIN storage_objects AS object ON object.id=version.storage_object_id
             {filter}
             ORDER BY version.created_at, version.id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params, parse_version)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn reconcile_identity(
        &self,
        case_id: &str,
        version: &StorageObjectVersion,
        native_json: &str,
    ) -> DbResult<()> {
        let conflicting: bool = self.conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM storage_object_versions
                 WHERE storage_object_id=?1
                   AND native_versions_json=?2
                   AND mapping_sha256<>?3
             )",
            params![
                version.storage_object_id,
                native_json,
                version.mapping_sha256
            ],
            |row| row.get(0),
        )?;
        if conflicting || version.consistency == RevisionConsistency::Conflicted {
            self.conn.execute(
                "UPDATE storage_objects
                 SET identity_state='conflicted', updated_at=datetime('now')
                 WHERE id=?1 AND case_id=?2 AND identity_state<>'conflicted'",
                params![version.storage_object_id, case_id],
            )?;
        } else {
            self.conn.execute(
                "UPDATE storage_objects
                 SET identity_state='proven', updated_at=datetime('now')
                 WHERE id=?1 AND case_id=?2 AND identity_state IN ('unproven', 'candidate')",
                params![version.storage_object_id, case_id],
            )?;
        }
        Ok(())
    }
}

fn parse_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageObjectVersion> {
    let native_object_json: String = row.get(2)?;
    let acquisition_json: String = row.get(8)?;
    let parent_json: String = row.get(9)?;
    let native_json: String = row.get(10)?;
    let consistency: String = row.get(11)?;
    let version_kind: String = row.get(12)?;
    Ok(StorageObjectVersion {
        id: ObjectVersionId(row.get(0)?),
        storage_object_id: row.get(1)?,
        native_object_id: parse_json::<NativeObjectId>(&native_object_json, "native object")?,
        owner_source_id: row.get(3)?,
        derived_source_id: row.get(4)?,
        adapter_version: row.get(5)?,
        policy_version: row.get(6)?,
        mapping_sha256: row.get(7)?,
        acquisition_ids: parse_json::<Vec<AcquisitionId>>(&acquisition_json, "acquisition IDs")?,
        parent_source_revisions: parse_json::<Vec<SourceRevisionRef>>(
            &parent_json,
            "parent revisions",
        )?,
        native_versions: parse_json(&native_json, "native versions")?,
        consistency: RevisionConsistency::parse(&consistency)
            .ok_or_else(|| conversion_error(11, "invalid storage version consistency"))?,
        version_kind: StorageObjectVersionKind::parse(&version_kind)
            .ok_or_else(|| conversion_error(12, "invalid storage version kind"))?,
    })
}

fn parse_json<T: serde::de::DeserializeOwned>(value: &str, field: &str) -> rusqlite::Result<T> {
    serde_json::from_str(value)
        .map_err(|error| conversion_error(0, &format!("invalid storage version {field}: {error}")))
}

fn conversion_error(column: usize, message: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.to_owned(),
        )),
    )
}

fn require_source_case(
    conn: &rusqlite::Connection,
    case_id: &str,
    source_id: &str,
) -> DbResult<()> {
    let source_case = DataSourceRepo::new(conn).case_id(&domain::DataSourceId(source_id.into()))?;
    if source_case.0 != case_id {
        return Err(DbError::System(
            "storage version source belongs to another case".into(),
        ));
    }
    Ok(())
}
