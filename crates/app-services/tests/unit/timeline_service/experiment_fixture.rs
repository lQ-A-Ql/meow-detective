use rusqlite::Connection;
use sha2::{Digest, Sha256};

pub(super) fn seed(connection: &Connection, events: u32) {
    connection.execute_batch(&format!(
        "INSERT INTO graph_nodes(id,case_id,node_type,label,created_at)
         VALUES ('file','case','file','target','2026-10-18T00:00:00Z');
         WITH RECURSIVE n(v) AS (VALUES(0) UNION ALL SELECT v+1 FROM n WHERE v<{})
         INSERT INTO timeline_events(id,case_id,source_object_id,event_type,ts,title,description,confidence,attrs)
         SELECT printf('event:%08d',v),'case','file','FILE_MODIFIED','2026-10-18T00:00:00Z',
                printf('event %08d',v),'source-bound event',0.75,'{{}}' FROM n;", events-1
    )).unwrap();
    crate::source_db::checkpoint_source_db(connection).unwrap();
}

pub(super) fn digest(connection: &Connection) -> String {
    let mut hash = Sha256::new();
    for query in [
        "SELECT json_array(id,case_id,node_type,label,summary,tags) FROM graph_nodes ORDER BY id",
        "SELECT json_array(id,case_id,source_id,target_id,edge_type,confidence,provenance) FROM graph_edges ORDER BY id",
    ] {
        let mut statement = connection.prepare(query).unwrap();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let value: String = row.get(0).unwrap();
            hash.update(value.as_bytes()); hash.update(b"\n");
        }
    }
    hex::encode(hash.finalize())
}
