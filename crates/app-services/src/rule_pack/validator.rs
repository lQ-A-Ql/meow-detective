use super::parser::{EdgeType, NodeType, Operator, RulePack};
use std::collections::HashSet;

/// Known artifact family values that can appear as `source_family` in rules.
///
/// This list mirrors the artifact types registered by the extractors and
/// should be kept in sync as new artifact families are added.
const KNOWN_SOURCE_FAMILIES: &[&str] = &[
    "LNK",
    "Prefetch",
    "Registry",
    "RegistryValue",
    "RecycleBin",
    "BrowserDownload",
    "BrowserHistory",
    "EmailMessage",
    "JumpList",
    "Shellbag",
    "USBDevice",
    "Amcache",
    "BAM",
    "MFTEntry",
    "EvtxEvent",
];

/// Validate a parsed rule pack against semantic constraints.
///
/// Returns a (possibly empty) list of validation error messages. An empty
/// list means the pack is valid.
pub fn validate_rule_pack(pack: &RulePack) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    // 1. Manifest checks
    if pack.manifest.name.trim().is_empty() {
        errors.push("manifest.name must not be empty".to_string());
    }
    if pack.manifest.version.trim().is_empty() {
        errors.push("manifest.version must not be empty".to_string());
    }
    if pack.manifest.scope.is_empty() {
        errors.push("manifest.scope must contain at least one scope value".to_string());
    }

    // 2. Duplicate rule ids
    let mut seen_ids = HashSet::new();
    for rule in &pack.rules {
        if rule.id.trim().is_empty() {
            errors.push("rule id must not be empty".to_string());
            continue;
        }
        if !seen_ids.insert(&rule.id) {
            errors.push(format!("duplicate rule id: '{}'", rule.id));
            continue;
        }

        // 3. Per-rule validation
        validate_rule(rule, &mut errors);
    }

    errors
}

fn validate_rule(rule: &super::parser::RuleDefinition, errors: &mut Vec<String>) {
    let prefix = format!("rule '{}'", rule.id);

    // source_family must be a known value
    if rule.source_family.trim().is_empty() {
        errors.push(format!("{prefix}: source_family must not be empty"));
    } else if !KNOWN_SOURCE_FAMILIES.contains(&rule.source_family.as_str()) {
        errors.push(format!(
            "{prefix}: unknown source_family '{}'. Known families: {:?}",
            rule.source_family, KNOWN_SOURCE_FAMILIES
        ));
    }

    // source_type vs target_type sanity
    if rule.source_type == rule.target_type {
        // This is a warning, not an error – self-referencing rules can be valid
        // (e.g., File→File temporal proximity)
    }

    // source_type must match source_family semantics
    match rule.source_type {
        NodeType::Artifact => {
            // Artifact source is the primary use case – ok
        }
        NodeType::TimelineEvent => {
            // TimelineEvent source is valid for temporal proximity rules
        }
        NodeType::File | NodeType::Entity | NodeType::Lead | NodeType::NotebookEntry => {
            // For now, only Artifact sources have defined source_family semantics.
            // Future rule packs may extend this.
        }
    }

    // edge_type must not be empty (the enum ensures this via serde)
    // Validate edge_type is not a nonsense reflexive edge
    if rule.source_type == rule.target_type && matches!(rule.edge_type, EdgeType::Contains) {
        errors.push(format!(
            "{prefix}: Contains edge from {:?} to {:?} is unusual",
            rule.source_type, rule.target_type
        ));
    }

    // Validate each condition's operator
    for (i, cond) in rule.conditions.iter().enumerate() {
        if cond.field.trim().is_empty() {
            errors.push(format!("{prefix}: condition[{i}].field must not be empty"));
        }
        if cond.target_field.trim().is_empty() {
            errors.push(format!(
                "{prefix}: condition[{i}].target_field must not be empty"
            ));
        }

        // TemporalProximity requires specific source/target types
        if cond.operator == Operator::TemporalProximity
            && rule.source_type != NodeType::TimelineEvent
            && rule.target_type != NodeType::TimelineEvent
        {
            errors.push(format!(
                "{prefix}: condition[{i}] uses temporal_proximity operator but neither source_type nor target_type is timeline_event"
            ));
        }
    }

    // Confidence must be a recognised level
    match rule.match_signals.confidence.as_str() {
        "direct" | "strong" | "weak" | "heuristic" => {}
        other => {
            errors.push(format!(
                "{prefix}: unknown confidence level '{}'. Expected one of: direct, strong, weak, heuristic",
                other
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_pack::parser::parse_rule_pack;

    const VALID_PACK_TOML: &str = r#"
[manifest]
name = "test-pack"
version = "1.0.0"
author = "test"
description = "Test pack"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "valid-rule"
name = "Valid Rule"
description = "A valid rule"
source_type = "artifact"
source_family = "LNK"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "target_path"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
"#;

    #[test]
    fn valid_pack_passes_validation() {
        let pack = parse_rule_pack(VALID_PACK_TOML).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn rejects_duplicate_rule_ids() {
        let toml = r#"
[manifest]
name = "dup"
version = "1.0.0"
author = "test"
description = "Duplicate ids"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "same-id"
name = "Rule A"
description = "First"
source_type = "artifact"
source_family = "LNK"
target_type = "file"
edge_type = "correlates_with"

[rules.match_signals]
confidence = "direct"

[[rules]]
id = "same-id"
name = "Rule B"
description = "Second"
source_type = "artifact"
source_family = "Prefetch"
target_type = "file"
edge_type = "correlates_with"

[rules.match_signals]
confidence = "strong"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("duplicate rule id")));
    }

    #[test]
    fn rejects_unknown_source_family() {
        let toml = r#"
[manifest]
name = "bad-family"
version = "1.0.0"
author = "test"
description = "Unknown family"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "r1"
name = "Bad"
description = "Bad"
source_type = "artifact"
source_family = "NonExistentFamily"
target_type = "file"
edge_type = "correlates_with"

[rules.match_signals]
confidence = "direct"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("unknown source_family")));
    }

    #[test]
    fn rejects_unknown_confidence_level() {
        let toml = r#"
[manifest]
name = "bad-conf"
version = "1.0.0"
author = "test"
description = "Bad confidence"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "r1"
name = "Bad"
description = "Bad"
source_type = "artifact"
source_family = "LNK"
target_type = "file"
edge_type = "correlates_with"

[rules.match_signals]
confidence = "certain"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.contains("unknown confidence level")));
    }

    #[test]
    fn rejects_empty_rule_id() {
        let toml = r#"
[manifest]
name = "empty-id"
version = "1.0.0"
author = "test"
description = "Empty rule id"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = ""
name = "Empty"
description = "Empty"
source_type = "artifact"
source_family = "LNK"
target_type = "file"
edge_type = "correlates_with"

[rules.match_signals]
confidence = "direct"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("must not be empty")));
    }

    #[test]
    fn rejects_empty_manifest_name() {
        let toml = r#"
[manifest]
name = ""
version = "1.0.0"
author = "test"
description = "Empty name"
scope = ["test"]
min_product_version = "0.1.0"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.contains("manifest.name must not be empty")));
    }

    #[test]
    fn rejects_empty_manifest_scope() {
        let toml = r#"
[manifest]
name = "no-scope"
version = "1.0.0"
author = "test"
description = "No scope"
scope = []
min_product_version = "0.1.0"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.contains("manifest.scope must contain at least one")));
    }

    #[test]
    fn warns_on_temporal_proximity_without_timeline_types() {
        let toml = r#"
[manifest]
name = "bad-temporal"
version = "1.0.0"
author = "test"
description = "Bad temporal proximity"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "r1"
name = "Bad"
description = "Bad"
source_type = "artifact"
source_family = "LNK"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "timestamp"
operator = "temporal_proximity"
target_field = "created_at"

[rules.match_signals]
confidence = "direct"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("temporal_proximity")));
    }
}
