use std::collections::HashSet;

use super::parser::{EdgeType, NodeType, Operator, RuleDefinition, RulePack};

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
pub fn validate_rule_pack(pack: &RulePack) -> Vec<String> {
    let mut errors = validate_manifest(pack);
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
        validate_rule(rule, &mut errors);
    }
    errors
}

fn validate_manifest(pack: &RulePack) -> Vec<String> {
    let mut errors = Vec::new();
    if pack.manifest.name.trim().is_empty() {
        errors.push("manifest.name must not be empty".to_string());
    }
    if pack.manifest.version.trim().is_empty() {
        errors.push("manifest.version must not be empty".to_string());
    }
    if pack.manifest.scope.is_empty() {
        errors.push("manifest.scope must contain at least one scope value".to_string());
    }
    errors
}

fn validate_rule(rule: &RuleDefinition, errors: &mut Vec<String>) {
    let prefix = format!("rule '{}'", rule.id);
    validate_family(rule, &prefix, errors);
    if rule.source_type == rule.target_type && matches!(rule.edge_type, EdgeType::Contains) {
        errors.push(format!(
            "{prefix}: Contains edge from {:?} to {:?} is unusual",
            rule.source_type, rule.target_type
        ));
    }
    validate_conditions(rule, &prefix, errors);
    if !matches!(
        rule.match_signals.confidence.as_str(),
        "direct" | "strong" | "weak" | "heuristic"
    ) {
        errors.push(format!(
            "{prefix}: unknown confidence level '{}'. Expected one of: direct, strong, weak, heuristic",
            rule.match_signals.confidence
        ));
    }
}

fn validate_family(rule: &RuleDefinition, prefix: &str, errors: &mut Vec<String>) {
    if rule.source_family.trim().is_empty() {
        errors.push(format!("{prefix}: source_family must not be empty"));
    } else if !KNOWN_SOURCE_FAMILIES.contains(&rule.source_family.as_str()) {
        errors.push(format!(
            "{prefix}: unknown source_family '{}'. Known families: {:?}",
            rule.source_family, KNOWN_SOURCE_FAMILIES
        ));
    }
}

fn validate_conditions(rule: &RuleDefinition, prefix: &str, errors: &mut Vec<String>) {
    for (index, condition) in rule.conditions.iter().enumerate() {
        if condition.field.trim().is_empty() {
            errors.push(format!(
                "{prefix}: condition[{index}].field must not be empty"
            ));
        }
        if condition.target_field.trim().is_empty() {
            errors.push(format!(
                "{prefix}: condition[{index}].target_field must not be empty"
            ));
        }
        if condition.operator == Operator::TemporalProximity
            && rule.source_type != NodeType::TimelineEvent
            && rule.target_type != NodeType::TimelineEvent
        {
            errors.push(format!(
                "{prefix}: condition[{index}] uses temporal_proximity operator but neither source_type nor target_type is timeline_event"
            ));
        }
    }
}
