//! Small helpers for building parameterized SQL fragments.
//!
//! Centralizes two patterns that were duplicated across repositories:
//! generating `?N` placeholder lists for `IN (...)` clauses, and
//! accumulating a list of conditional `WHERE`/`SET` fragments together with
//! correctly numbered bound parameters.

use rusqlite::types::ToSql;

/// Build a comma-joined `?N, ?N+1, ..., ?N+count-1` placeholder list, e.g.
/// `placeholders(1, 3)` produces `"?1, ?2, ?3"`. Used for `IN (...)` clauses
/// where the number of bound values is only known at call time.
pub fn placeholders(start: usize, count: usize) -> String {
    (start..start + count)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Accumulates conditional SQL fragments (`WHERE`-style `AND` clauses or
/// `UPDATE ... SET` assignments) together with their bound parameters,
/// keeping `?N` placeholder numbering consistent as fragments are added.
#[derive(Default)]
pub struct ClauseBuilder {
    clauses: Vec<String>,
    params: Vec<Box<dyn ToSql>>,
}

impl ClauseBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The 1-based parameter index the next pushed value will receive.
    pub fn next_param(&self) -> usize {
        self.params.len() + 1
    }

    /// Bind a value without adding a clause fragment; returns its 1-based
    /// placeholder index. Used when a parameter (e.g. a trailing `WHERE id =
    /// ?N`) isn't part of the accumulated `SET`/`AND` clause list.
    pub fn push_param<T: ToSql + 'static>(&mut self, value: T) -> usize {
        let idx = self.next_param();
        self.params.push(Box::new(value));
        idx
    }

    /// Push a `column = ?N` equality clause.
    pub fn push_eq<T: ToSql + 'static>(&mut self, column: &str, value: T) -> &mut Self {
        let idx = self.push_param(value);
        self.clauses.push(format!("{column} = ?{idx}"));
        self
    }

    /// Push a `column {op} ?N` comparison clause (e.g. `op` = `">="`, `"<="`, `"LIKE"`).
    pub fn push_cmp<T: ToSql + 'static>(&mut self, column: &str, op: &str, value: T) -> &mut Self {
        let idx = self.push_param(value);
        self.clauses.push(format!("{column} {op} ?{idx}"));
        self
    }

    /// Push a caller-built clause fragment (already containing its own `?N`
    /// placeholders obtained via [`ClauseBuilder::next_param`]) along with
    /// the values it binds, in order. Covers shapes `push_eq`/`push_cmp`
    /// don't, such as multi-column `OR` groups.
    pub fn push_raw<T: ToSql + 'static>(&mut self, clause: String, values: Vec<T>) -> &mut Self {
        self.clauses.push(clause);
        for value in values {
            self.params.push(Box::new(value));
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    /// Render accumulated clauses as a `WHERE ...` fragment (`AND`-joined),
    /// or an empty string if nothing was pushed.
    pub fn where_clause(&self) -> String {
        if self.clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.clauses.join(" AND "))
        }
    }

    /// Render accumulated clauses as comma-joined `SET` assignments.
    pub fn set_clause(&self) -> String {
        self.clauses.join(", ")
    }

    /// Borrowed `&dyn ToSql` references suitable for `query_map`/`execute`.
    pub fn param_refs(&self) -> Vec<&dyn ToSql> {
        self.params.iter().map(|p| p.as_ref()).collect()
    }

    pub fn into_params(self) -> Vec<Box<dyn ToSql>> {
        self.params
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_generates_sequential_list() {
        assert_eq!(placeholders(1, 3), "?1, ?2, ?3");
        assert_eq!(placeholders(4, 2), "?4, ?5");
        assert_eq!(placeholders(1, 0), "");
    }

    #[test]
    fn clause_builder_tracks_param_indices() {
        let mut builder = ClauseBuilder::new();
        builder.push_eq("case_id", "case-1".to_string());
        builder.push_cmp("ts", ">=", "2025-01-01".to_string());
        assert_eq!(builder.where_clause(), "WHERE case_id = ?1 AND ts >= ?2");
        assert_eq!(builder.param_refs().len(), 2);
    }

    #[test]
    fn empty_builder_produces_empty_where_clause() {
        let builder = ClauseBuilder::new();
        assert!(builder.is_empty());
        assert_eq!(builder.where_clause(), "");
    }

    #[test]
    fn set_clause_joins_with_commas() {
        let mut builder = ClauseBuilder::new();
        builder.push_eq("title", "New title".to_string());
        builder.push_eq("updated_at", "2025-01-01".to_string());
        assert_eq!(builder.set_clause(), "title = ?1, updated_at = ?2");
    }

    #[test]
    fn push_raw_appends_clause_and_values_after_existing_params() {
        let mut builder = ClauseBuilder::new();
        builder.push_eq("case_id", "case-1".to_string());
        let idx = builder.next_param();
        builder.push_raw(
            format!("(title LIKE ?{idx} OR body LIKE ?{})", idx + 1),
            vec!["%a%".to_string(), "%a%".to_string()],
        );
        assert_eq!(
            builder.where_clause(),
            "WHERE case_id = ?1 AND (title LIKE ?2 OR body LIKE ?3)"
        );
        assert_eq!(builder.param_refs().len(), 3);
    }
}
