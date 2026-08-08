use rusqlite::params;

use crate::connection::{DbError, DbResult};

use super::{
    DeletedRecoveryHashAlgorithm, DeletedRecoveryRecord, DeletedRecoveryRepo, RecoveryScanRecord,
};

const MAX_HASH_RESULTS: u32 = 100;

impl DeletedRecoveryRepo<'_> {
    pub fn search_by_hash(
        &self,
        data_source_id: &str,
        algorithm: DeletedRecoveryHashAlgorithm,
        digest: &str,
        limit: u32,
    ) -> DbResult<Vec<(RecoveryScanRecord, DeletedRecoveryRecord)>> {
        if limit == 0 || limit > MAX_HASH_RESULTS {
            return Err(DbError::System(
                "recovery hash result limit must be between 1 and 100".to_string(),
            ));
        }
        let (column, expected_len) = match algorithm {
            DeletedRecoveryHashAlgorithm::Md5 => ("content_md5", 32),
            DeletedRecoveryHashAlgorithm::Sha1 => ("content_sha1", 40),
            DeletedRecoveryHashAlgorithm::Sha256 => ("content_sha256", 64),
        };
        if digest.len() != expected_len
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(DbError::System(
                "recovery hash must be a normalized hexadecimal digest".to_string(),
            ));
        }
        let sql = format!(
            "SELECT recovery.id
             FROM deleted_file_recoveries AS recovery
             INNER JOIN filesystem_recovery_scans AS scan ON scan.id = recovery.scan_id
             WHERE scan.data_source_id = ?1 AND recovery.{column} = ?2
             ORDER BY scan.partition_index, CAST(recovery.inode AS INTEGER), recovery.id
             LIMIT ?3"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let ids = statement
            .query_map(params![data_source_id, digest, limit], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        ids.into_iter()
            .map(|id| {
                self.find_recovery(data_source_id, &id)?.ok_or_else(|| {
                    DbError::System("recovery hash index returned a missing record".to_string())
                })
            })
            .collect()
    }
}
