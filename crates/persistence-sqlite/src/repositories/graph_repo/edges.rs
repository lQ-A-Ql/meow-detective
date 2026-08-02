use crate::connection::DbResult;
use domain::GraphEdge;
use rusqlite::params;

use super::mapping::EDGE_COLUMNS;
use super::{mapping::row_to_edge, GraphRepo};

impl GraphRepo<'_> {
    /// Find a graph edge by its id.
    pub fn find_edge_by_id(&self, edge_id: &str) -> DbResult<Option<GraphEdge>> {
        let sql = format!("SELECT {EDGE_COLUMNS} FROM graph_edges WHERE id = ?1");
        let mut statement = self.conn.prepare(&sql)?;
        match statement.query_row(params![edge_id], row_to_edge) {
            Ok(edge) => Ok(Some(edge)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Find edge provenance values for incremental rule-pack execution.
    pub fn find_edges_with_provenance_by_case(
        &self,
        case_id: &str,
        edge_type: &str,
    ) -> DbResult<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT DISTINCT provenance FROM graph_edges
             WHERE case_id = ?1 AND edge_type = ?2
             AND provenance IS NOT NULL",
        )?;
        let rows = statement.query_map(params![case_id, edge_type], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
