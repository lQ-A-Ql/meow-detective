use persistence_sqlite::DbResult;
use rusqlite::Connection;

pub(super) fn validate_enum_staging(conn: &Connection) -> DbResult<()> {
    conn.prepare(
        "SELECT id,parent_id,data_source_id,path,name,entry_type,size,ext,deleted,hidden,system,
         encrypted,read_only,archive,unix_mode,partition_index,created_at,modified_at,accessed_at,
         changed_at,hash_sha256 FROM file_entries LIMIT 0",
    )?;
    conn.prepare("SELECT key,value FROM staging_meta LIMIT 0")?;
    Ok(())
}

pub(super) fn validate_analysis_staging(conn: &Connection) -> DbResult<()> {
    conn.prepare(
        "SELECT id,file_id,artifact_type,extractor_id,extractor_version,confidence,
         source_attribution,display_name,summary,data_json,source_path,created_at
         FROM artifact_rows LIMIT 0",
    )?;
    conn.prepare(
        "SELECT id,file_id,timestamp,event_type,parser_id,parser_version,confidence,
         source_attribution,title,description,data_json FROM timeline_rows LIMIT 0",
    )?;
    conn.prepare("SELECT file_id,path,text,language,truncated FROM index_docs LIMIT 0")?;
    conn.prepare("SELECT key,value FROM worker_meta LIMIT 0")?;
    Ok(())
}
