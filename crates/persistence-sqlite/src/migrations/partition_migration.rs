//! 分区数据迁移
//!
//! 将 data_sources.partitions JSON 数据迁移到 partitions 表。

use crate::connection::DbResult;
use crate::repositories::partition_repo::{DataSourcePartitionRecord, PartitionRepo};
use rusqlite::Connection;
use uuid::Uuid;

/// 迁移结果
#[derive(Debug)]
pub struct MigrationResult {
    pub migrated_count: u64,
    pub skipped_count: u64,
    pub error_count: u64,
    pub errors: Vec<String>,
}

/// 执行分区数据迁移
pub fn migrate_partitions(conn: &Connection) -> DbResult<MigrationResult> {
    let mut result = MigrationResult {
        migrated_count: 0,
        skipped_count: 0,
        error_count: 0,
        errors: Vec::new(),
    };

    // 检查 data_sources 表是否有 partitions 列
    let has_partitions_column = check_partitions_column(conn)?;
    if !has_partitions_column {
        result.skipped_count = 1;
        return Ok(result);
    }

    // 获取所有有分区数据的数据源
    let data_sources = get_data_sources_with_partitions(conn)?;

    let repo = PartitionRepo::new(conn);

    for (ds_id, partitions_json) in data_sources {
        match parse_and_migrate_partitions(&repo, &ds_id, &partitions_json) {
            Ok(count) => {
                result.migrated_count += count;
            }
            Err(e) => {
                result.error_count += 1;
                result.errors.push(format!("Error migrating {}: {}", ds_id, e));
            }
        }
    }

    // 更新迁移日志
    update_migration_log(conn, &result)?;

    Ok(result)
}

/// 检查 data_sources 表是否有 partitions 列
fn check_partitions_column(conn: &Connection) -> DbResult<bool> {
    let mut stmt = conn.prepare("PRAGMA table_info(data_sources)")?;
    let has_column = stmt
        .query_map([], |row| {
            let name: String = row.get(1)?;
            Ok(name)
        })?
        .filter_map(|r| r.ok())
        .any(|name| name == "partitions");

    Ok(has_column)
}

/// 获取有分区数据的数据源
fn get_data_sources_with_partitions(conn: &Connection) -> DbResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, partitions FROM data_sources WHERE partitions IS NOT NULL AND partitions != '[]'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// 解析 JSON 并迁移分区数据
fn parse_and_migrate_partitions(
    repo: &PartitionRepo,
    ds_id: &str,
    partitions_json: &str,
) -> Result<u64, String> {
    let partitions: Vec<serde_json::Value> =
        serde_json::from_str(partitions_json).map_err(|e| format!("JSON parse error: {}", e))?;

    let mut records = Vec::new();

    for (index, partition) in partitions.iter().enumerate() {
        let id = Uuid::new_v4().to_string();
        let name = partition["name"]
            .as_str()
            .unwrap_or(&format!("Partition {}", index + 1))
            .to_string();
        let kind_label = partition["kind_label"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();
        let status = partition["status"]
            .as_str()
            .unwrap_or("unsupported")
            .to_string();
        let type_guid = partition["type_guid"].as_str().map(|s| s.to_string());
        let offset = partition["offset"].as_u64().unwrap_or(0);
        let length = partition["length"].as_u64().unwrap_or(0);
        let filesystem = partition["filesystem"].as_str().map(|s| s.to_string());
        let unlock_hint = partition["unlock_hint"].as_str().map(|s| s.to_string());

        records.push(DataSourcePartitionRecord {
            id,
            data_source_id: ds_id.to_string(),
            partition_index: index as u32,
            name,
            kind_label,
            status,
            type_guid,
            offset,
            length,
            filesystem,
            unlock_hint,
        });
    }

    let count = records.len() as u64;
    if !records.is_empty() {
        repo.insert_batch(&records)
            .map_err(|e| format!("Insert error: {}", e))?;
    }

    Ok(count)
}

/// 更新迁移日志
fn update_migration_log(conn: &Connection, result: &MigrationResult) -> DbResult<()> {
    let status = if result.error_count > 0 {
        "partial"
    } else {
        "completed"
    };
    let details = format!(
        "Migrated: {}, Skipped: {}, Errors: {}",
        result.migrated_count, result.skipped_count, result.error_count
    );

    conn.execute(
        "UPDATE migration_log SET status = ?1, details = ?2 WHERE migration_name = '0012_migrate_partitions'",
        rusqlite::params![status, details],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_migrate() {
        let conn = crate::connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE data_sources (id TEXT PRIMARY KEY, case_id TEXT, name TEXT, kind TEXT, source_path TEXT, imported_at TEXT);
             CREATE TABLE data_source_partitions (id TEXT PRIMARY KEY, data_source_id TEXT REFERENCES data_sources(id), partition_index INTEGER, name TEXT, kind_label TEXT, status TEXT, type_guid TEXT, offset INTEGER, length INTEGER, filesystem TEXT, unlock_hint TEXT);"
        ).unwrap();

        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES ('ds1', 'c1', 'Test', 'E01', '/path', '2024-01-01')",
            [],
        ).unwrap();

        let repo = PartitionRepo::new(&conn);
        let json = r#"[{"name":"Partition 1","kind_label":"NTFS","status":"supported","offset":0,"length":1048576,"filesystem":"NTFS"}]"#;

        let count = parse_and_migrate_partitions(&repo, "ds1", json).unwrap();
        assert_eq!(count, 1);

        let records = repo.find_by_data_source("ds1").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "Partition 1");
    }
}
