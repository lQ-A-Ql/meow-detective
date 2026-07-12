//! Rule-pack TOML deserialization facade.

pub use super::builtin::V2_STANDARD_TOML;
pub use super::model::{
    EdgeType, FieldPredicate, MatchSignals, NodeType, Operator, RuleDefinition, RulePack,
    RulePackManifest,
};

/// Parse TOML into a rule pack. Semantic validation is a separate step.
pub fn parse_rule_pack(toml_str: &str) -> Result<RulePack, Vec<String>> {
    toml::from_str::<RulePack>(toml_str).map_err(|error| {
        error
            .to_string()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    })
}
