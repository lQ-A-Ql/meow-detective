use crate::connection::DbResult;
use domain::DataSourceId;
use rusqlite::{params, Connection};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AndroidFactRecord {
    pub field: String,
    pub value: String,
    pub confidence: String,
    pub source_file_id: String,
    pub source_path: String,
    pub source_rank: i64,
    pub parser: String,
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AndroidPackageRecord {
    pub package_name: String,
    pub app_name: Option<String>,
    pub version_code: Option<String>,
    pub install_time: Option<String>,
    pub update_time: Option<String>,
    pub installer: Option<String>,
    pub uid: Option<u32>,
    pub code_path: Option<String>,
    pub source_file_id: String,
    pub source_path: String,
    pub parser: String,
    pub warning: Option<String>,
}

pub struct AndroidAnalysisRepo<'a> {
    conn: &'a Connection,
}

impl<'a> AndroidAnalysisRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn replace_for_source(
        &self,
        data_source_id: &DataSourceId,
        facts: &[AndroidFactRecord],
        packages: &[AndroidPackageRecord],
    ) -> DbResult<()> {
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM android_system_facts WHERE data_source_id = ?1",
            [data_source_id.0.as_str()],
        )?;
        transaction.execute(
            "DELETE FROM android_packages WHERE data_source_id = ?1",
            [data_source_id.0.as_str()],
        )?;
        for fact in facts {
            transaction.execute(
                "INSERT INTO android_system_facts
                 (id, data_source_id, field_key, field_value, confidence, source_file_id, source_path, parser, source_rank, warning)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    Uuid::new_v4().to_string(),
                    &data_source_id.0,
                    &fact.field,
                    &fact.value,
                    &fact.confidence,
                    &fact.source_file_id,
                    &fact.source_path,
                    &fact.parser,
                    fact.source_rank,
                    &fact.warning,
                ],
            )?;
        }
        for package in packages {
            transaction.execute(
                "INSERT INTO android_packages
                 (id, data_source_id, package_name, app_name, version_code, install_time, update_time, installer, uid, code_path, source_file_id, source_path, parser, warning)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    Uuid::new_v4().to_string(),
                    &data_source_id.0,
                    &package.package_name,
                    &package.app_name,
                    &package.version_code,
                    &package.install_time,
                    &package.update_time,
                    &package.installer,
                    package.uid,
                    &package.code_path,
                    &package.source_file_id,
                    &package.source_path,
                    &package.parser,
                    &package.warning,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_facts(&self, data_source_id: &DataSourceId) -> DbResult<Vec<AndroidFactRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT field_key, field_value, confidence, source_file_id, source_path, parser, source_rank, warning
             FROM android_system_facts
             WHERE data_source_id = ?1
             ORDER BY source_rank ASC, field_key ASC",
        )?;
        let rows = statement.query_map([data_source_id.0.as_str()], |row| {
            Ok(AndroidFactRecord {
                field: row.get(0)?,
                value: row.get(1)?,
                confidence: row.get(2)?,
                source_file_id: row.get(3)?,
                source_path: row.get(4)?,
                parser: row.get(5)?,
                source_rank: row.get(6)?,
                warning: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn count_packages(&self, data_source_id: &DataSourceId) -> DbResult<u64> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM android_packages WHERE data_source_id = ?1",
            [data_source_id.0.as_str()],
            |row| row.get::<_, u64>(0),
        )?;
        Ok(count)
    }

    pub fn list_packages_page(
        &self,
        data_source_id: &DataSourceId,
        offset: u64,
        limit: u32,
    ) -> DbResult<Vec<AndroidPackageRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT package_name, app_name, version_code, install_time, update_time, installer, uid,
                    code_path, source_file_id, source_path, parser, warning
             FROM android_packages
             WHERE data_source_id = ?1
             ORDER BY package_name COLLATE NOCASE ASC
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows =
            statement.query_map(params![data_source_id.0.as_str(), limit, offset], |row| {
                Ok(AndroidPackageRecord {
                    package_name: row.get(0)?,
                    app_name: row.get(1)?,
                    version_code: row.get(2)?,
                    install_time: row.get(3)?,
                    update_time: row.get(4)?,
                    installer: row.get(5)?,
                    uid: row.get(6)?,
                    code_path: row.get(7)?,
                    source_file_id: row.get(8)?,
                    source_path: row.get(9)?,
                    parser: row.get(10)?,
                    warning: row.get(11)?,
                })
            })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
