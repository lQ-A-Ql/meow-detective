use rusqlite::Connection;

use super::super::error::RulePackError;
use super::super::parser::RulePack;
use super::{execution, projection};

/// Execute every rule in a pack in declaration order.
pub fn execute_rule_pack(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
) -> Result<u64, RulePackError> {
    execute_selected_rules(conn, case_id, pack, |_| true)
}

/// Execute rules that have not already produced provenance for this pack.
pub fn execute_rule_pack_incremental(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
) -> Result<u64, RulePackError> {
    let executed = projection::executed_rule_ids(conn, case_id, &pack.manifest.name)?;
    execute_selected_rules(conn, case_id, pack, |rule_id| !executed.contains(rule_id))
}

fn execute_selected_rules<F>(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
    should_execute: F,
) -> Result<u64, RulePackError>
where
    F: Fn(&str) -> bool,
{
    let mut total_edges = 0;
    for rule in &pack.rules {
        if should_execute(&rule.id) {
            total_edges += execution::execute_rule(conn, case_id, pack, rule)?;
        }
    }
    Ok(total_edges)
}
