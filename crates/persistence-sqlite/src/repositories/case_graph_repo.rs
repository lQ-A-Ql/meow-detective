use crate::{
    connection::{DbError, DbResult},
    repositories::graph_repo::GraphRepo,
};
use domain::{GraphEdge, GraphNode};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseGraphProjection {
    pub case_id: String,
    pub projection_version: String,
    pub source_manifest: String,
    pub built_at: String,
    pub source_count: u32,
    pub cross_source_entity_count: u64,
    pub cross_source_edge_count: u64,
    pub seed_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseGraphSourceState {
    pub data_source_id: String,
    pub schema_version: String,
    pub database_size_bytes: u64,
    pub database_modified_ns: String,
    pub wal_size_bytes: u64,
    pub wal_modified_ns: String,
}

pub struct CaseGraphRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CaseGraphRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get_projection(&self) -> DbResult<Option<CaseGraphProjection>> {
        let row = self
            .conn
            .query_row(
                "SELECT case_id, projection_version, source_manifest, built_at,
                        source_count, cross_source_entity_count,
                        cross_source_edge_count, seed_ids_json
                 FROM case_graph_projection WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, u32>(4)?,
                        row.get::<_, u64>(5)?,
                        row.get::<_, u64>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(
                case_id,
                projection_version,
                source_manifest,
                built_at,
                source_count,
                cross_source_entity_count,
                cross_source_edge_count,
                seed_ids_json,
            )| {
                let seed_ids = serde_json::from_str(&seed_ids_json).map_err(|error| {
                    DbError::System(format!("Invalid case graph seed metadata: {error}"))
                })?;
                Ok(CaseGraphProjection {
                    case_id,
                    projection_version,
                    source_manifest,
                    built_at,
                    source_count,
                    cross_source_entity_count,
                    cross_source_edge_count,
                    seed_ids,
                })
            },
        )
        .transpose()
    }

    pub fn replace_projection(
        &self,
        projection: &CaseGraphProjection,
        sources: &[CaseGraphSourceState],
        nodes: &[GraphNode],
        edges: &[GraphEdge],
    ) -> DbResult<()> {
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute("DELETE FROM graph_edges", [])?;
        transaction.execute("DELETE FROM graph_nodes", [])?;
        transaction.execute("DELETE FROM case_graph_sources", [])?;
        transaction.execute("DELETE FROM case_graph_projection", [])?;

        let graph_repo = GraphRepo::new(&transaction);
        graph_repo.insert_nodes_batch_unchecked(nodes)?;
        graph_repo.insert_edges_batch_unchecked(edges)?;

        {
            let mut statement = transaction.prepare_cached(
                "INSERT INTO case_graph_sources
                 (data_source_id, schema_version, database_size_bytes,
                  database_modified_ns, wal_size_bytes, wal_modified_ns)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for source in sources {
                statement.execute(params![
                    source.data_source_id,
                    source.schema_version,
                    source.database_size_bytes,
                    source.database_modified_ns,
                    source.wal_size_bytes,
                    source.wal_modified_ns,
                ])?;
            }
        }

        let seed_ids_json = serde_json::to_string(&projection.seed_ids)
            .map_err(|error| DbError::System(format!("Serialize case graph seeds: {error}")))?;
        transaction.execute(
            "INSERT INTO case_graph_projection
             (singleton, case_id, projection_version, source_manifest, built_at,
              source_count, cross_source_entity_count, cross_source_edge_count,
              seed_ids_json)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                projection.case_id,
                projection.projection_version,
                projection.source_manifest,
                projection.built_at,
                projection.source_count,
                projection.cross_source_entity_count,
                projection.cross_source_edge_count,
                seed_ids_json,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/case_graph_repo.rs"]
mod tests;
