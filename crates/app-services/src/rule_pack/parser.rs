use domain;
use serde::{Deserialize, Serialize};

// ── TOML-serializable enums (snake_case for rule pack authoring) ──

/// Serializable node type mirroring domain::NodeType with snake_case TOML keys.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    File,
    Artifact,
    TimelineEvent,
    Entity,
    Lead,
    NotebookEntry,
}

impl From<NodeType> for domain::NodeType {
    fn from(nt: NodeType) -> Self {
        match nt {
            NodeType::File => domain::NodeType::File,
            NodeType::Artifact => domain::NodeType::Artifact,
            NodeType::TimelineEvent => domain::NodeType::TimelineEvent,
            NodeType::Entity => domain::NodeType::Entity,
            NodeType::Lead => domain::NodeType::Lead,
            NodeType::NotebookEntry => domain::NodeType::NotebookEntry,
        }
    }
}

/// Serializable edge type mirroring domain::EdgeType with snake_case TOML keys.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Contains,
    References,
    CorrelatesWith,
    DerivesFrom,
    Precedes,
    Cites,
    Annotates,
}

impl From<EdgeType> for domain::EdgeType {
    fn from(et: EdgeType) -> Self {
        match et {
            EdgeType::Contains => domain::EdgeType::Contains,
            EdgeType::References => domain::EdgeType::References,
            EdgeType::CorrelatesWith => domain::EdgeType::CorrelatesWith,
            EdgeType::DerivesFrom => domain::EdgeType::DerivesFrom,
            EdgeType::Precedes => domain::EdgeType::Precedes,
            EdgeType::Cites => domain::EdgeType::Cites,
            EdgeType::Annotates => domain::EdgeType::Annotates,
        }
    }
}

/// Comparison operator for field predicates in a rule condition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    /// Exact string equality.
    Equals,
    /// Case-insensitive substring match.
    Contains,
    /// Regular expression match (Rust regex syntax).
    Regex,
    /// Normalised path equality (backslash→forward slash, case-insensitive).
    PathEquals,
    /// Basename equality (last path segment, case-insensitive).
    FilenameEquals,
    /// Temporal proximity window match (reserved for future timeline rules).
    TemporalProximity,
}

// ── Rule pack structs (deserialized directly from TOML) ──

/// Manifest metadata for a rule pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackManifest {
    /// Human-readable pack name.
    pub name: String,
    /// Semantic version of the pack.
    pub version: String,
    /// Pack author or organisation.
    pub author: String,
    /// Free-form description of what the pack covers.
    pub description: String,
    /// Usage scopes (e.g. "correlation", "investigation", "triage").
    pub scope: Vec<String>,
    /// Minimum product version required to execute this pack.
    pub min_product_version: String,
    /// Global caveats that apply to all rules in this pack.
    #[serde(default)]
    pub caveats: Vec<String>,
}

/// Confidence and caveat signals produced when a rule matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSignals {
    /// Confidence level: "direct", "strong", "weak", "heuristic".
    pub confidence: String,
    /// Rule-specific caveat to surface alongside the match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caveats: Option<String>,
}

/// A single field-level predicate in a rule condition.
///
/// Compares a source attribute `field` against a target attribute `target_field`
/// using the given `operator`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPredicate {
    /// Attribute name on the source node (e.g. "target_path", "executable").
    pub field: String,
    /// Comparison operator.
    pub operator: Operator,
    /// Attribute name on the target node (e.g. "path", "name").
    pub target_field: String,
}

/// A single correlation rule that links source nodes to target nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleDefinition {
    /// Unique rule identifier within the pack.
    pub id: String,
    /// Human-readable rule name.
    pub name: String,
    /// Description of what the rule detects.
    pub description: String,
    /// Type of source node (e.g. "artifact").
    pub source_type: NodeType,
    /// Family/artifact-type of the source (e.g. "LNK", "Prefetch").
    pub source_family: String,
    /// Type of target node (e.g. "file").
    pub target_type: NodeType,
    /// Type of edge to create between source and target.
    pub edge_type: EdgeType,
    /// Field predicates that must be satisfied for a match.
    #[serde(default)]
    pub conditions: Vec<FieldPredicate>,
    /// Confidence and caveat signals for match output.
    pub match_signals: MatchSignals,
}

/// A complete rule pack with manifest and rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePack {
    pub manifest: RulePackManifest,
    #[serde(default)]
    pub rules: Vec<RuleDefinition>,
}

// ── Public API ──

/// Parse a TOML rule pack string into a structured [`RulePack`].
///
/// Returns the parsed pack on success, or a list of TOML syntax errors on failure.
/// Call [`super::validator::validate_rule_pack`] after parsing to check semantic
/// validity (known families, valid operators, no duplicate ids, etc.).
pub fn parse_rule_pack(toml_str: &str) -> Result<RulePack, Vec<String>> {
    toml::from_str::<RulePack>(toml_str).map_err(|e| {
        // Split multi-line TOML errors into individual messages
        e.to_string()
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
    })
}

// ── Built-in rule pack fixtures ──

/// Built-in V2 correlation rules expressed as a TOML rule pack.
///
/// This constant contains the canonical v2-standard rule pack used as the
/// default correlation rule set. It is public so that other modules (e.g.,
/// the engine tests) can reference it.
pub const V2_STANDARD_TOML: &str = r#"
[manifest]
name = "v2-standard"
version = "1.0.0"
author = "Forensics Workbench"
description = "Built-in V2 correlation rules mapping artifacts to files"
scope = ["correlation", "investigation"]
min_product_version = "0.1.0"
caveats = [
    "Path-based matches depend on artifact field normalization",
    "Name-based matches may hit unrelated files with the same basename"
]

# ── LNK: target_path → file path ──
[[rules]]
id = "lnk-path-match"
name = "LNK Target Path Match"
description = "Match LNK artifact target_path to file entries by exact path"
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
caveats = "May need to review original LNK target_path content"

# ── Prefetch: executable basename → file name ──
[[rules]]
id = "prefetch-name-match"
name = "Prefetch Executable Name Match"
description = "Match Prefetch artifact executable basename to file entries by name"
source_type = "artifact"
source_family = "Prefetch"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "executable"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "strong"
caveats = "Name match may hit files outside the expected binary directory"

# ── Registry: data contains path → file path ──
[[rules]]
id = "registry-path-match"
name = "Registry Value Path Match"
description = "Match Registry artifact data containing a file path to file entries by path"
source_type = "artifact"
source_family = "RegistryValue"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "data"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "strong"
caveats = "Registry value data may contain env vars or CLI args; review original value"

# ── RecycleBin: original_path → deleted file path ──
[[rules]]
id = "recycle-bin-original-path-match"
name = "Recycle Bin Original Path Match"
description = "Match Recycle Bin artifact original_path to deleted file entries by path"
source_type = "artifact"
source_family = "RecycleBin"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "original_path"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
caveats = "Original path reflects pre-deletion path; verify deletion time aligns"

# ── BrowserDownload: targetPath → file path ──
[[rules]]
id = "browser-download-path-match"
name = "Browser Download Target Path Match"
description = "Match BrowserDownload artifact targetPath to file entries by path"
source_type = "artifact"
source_family = "BrowserDownload"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "targetPath"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
caveats = "Download path from browser DB; verify with file content and timeline"

# ── BrowserHistory: title or url → file name ──
[[rules]]
id = "browser-history-title-name-match"
name = "Browser History Title Name Match"
description = "Match BrowserHistory artifact title to file entries by name"
source_type = "artifact"
source_family = "BrowserHistory"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "title"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
caveats = "Title-based name match is weak; verify with visit time and URL context"

[[rules]]
id = "browser-history-url-name-match"
name = "Browser History URL Name Match"
description = "Match BrowserHistory artifact URL path segment to file entries by name"
source_type = "artifact"
source_family = "BrowserHistory"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "url"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
caveats = "URL path segment name match is weak; verify with visit time and title"

# ── EmailMessage: attachment name → file name ──
[[rules]]
id = "email-attachment-name-match"
name = "Email Attachment Name Match"
description = "Match EmailMessage artifact attachment names to file entries by name"
source_type = "artifact"
source_family = "EmailMessage"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "attachments"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
caveats = "Attachment name match is weak; verify with sentAt, subject, and content"

# ── EmailMessage: subject → file name ──
[[rules]]
id = "email-subject-name-match"
name = "Email Subject Name Match"
description = "Match EmailMessage artifact subject to file entries by name"
source_type = "artifact"
source_family = "EmailMessage"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "subject"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
caveats = "Subject name match is weak; verify with sentAt and attachment context"

# ── JumpList: target_path → file path ──
[[rules]]
id = "jumplist-path-match"
name = "JumpList Target Path Match"
description = "Match JumpList artifact target_path to file entries by path"
source_type = "artifact"
source_family = "JumpList"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "target_path"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
caveats = "JumpList match depends on embedded LNK extraction quality"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test fixtures ──

    /// Minimal valid rule pack for testing.
    const MINIMAL_TOML: &str = r#"
[manifest]
name = "minimal"
version = "0.1.0"
author = "test"
description = "Minimal test pack"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "test-rule"
name = "Test Rule"
description = "A test rule"
source_type = "artifact"
source_family = "Test"
target_type = "file"
edge_type = "references"

[[rules.conditions]]
field = "test_field"
operator = "equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
"#;

    // ── Tests ──

    #[test]
    fn parse_valid_v2_standard_pack() {
        let pack = parse_rule_pack(V2_STANDARD_TOML).unwrap();
        assert_eq!(pack.manifest.name, "v2-standard");
        assert_eq!(pack.manifest.version, "1.0.0");
        assert!(pack.manifest.scope.contains(&"correlation".to_string()));
        assert_eq!(pack.manifest.caveats.len(), 2);
        assert_eq!(pack.rules.len(), 10);
        assert_eq!(pack.rules[0].id, "lnk-path-match");
        assert_eq!(pack.rules[0].source_type, NodeType::Artifact);
        assert_eq!(pack.rules[0].source_family, "LNK");
        assert_eq!(pack.rules[0].target_type, NodeType::File);
        assert_eq!(pack.rules[0].edge_type, EdgeType::CorrelatesWith);
        assert_eq!(pack.rules[0].conditions.len(), 1);
        assert_eq!(pack.rules[0].conditions[0].field, "target_path");
        assert_eq!(pack.rules[0].conditions[0].operator, Operator::PathEquals);
        assert_eq!(pack.rules[0].match_signals.confidence, "direct");
        assert!(pack.rules[0].match_signals.caveats.is_some());
    }

    #[test]
    fn parse_minimal_valid_pack() {
        let pack = parse_rule_pack(MINIMAL_TOML).unwrap();
        assert_eq!(pack.manifest.name, "minimal");
        assert_eq!(pack.rules.len(), 1);
        assert_eq!(pack.rules[0].id, "test-rule");
    }

    #[test]
    fn reject_invalid_toml_syntax() {
        let result = parse_rule_pack("this is not valid toml {{{");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn reject_missing_required_manifest_fields() {
        let toml_str = r#"
[manifest]
name = "incomplete"
# missing version, author, description, scope, min_product_version
"#;
        let result = parse_rule_pack(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_rules_is_valid() {
        let toml_str = r#"
[manifest]
name = "no-rules"
version = "1.0.0"
author = "test"
description = "Pack with no rules"
scope = ["test"]
min_product_version = "0.1.0"
"#;
        let pack = parse_rule_pack(toml_str).unwrap();
        assert!(pack.rules.is_empty());
    }

    #[test]
    fn parse_match_signals_without_caveats() {
        let toml_str = r#"
[manifest]
name = "no-caveat"
version = "1.0.0"
author = "test"
description = "Test"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "r1"
name = "Rule 1"
description = "No caveat"
source_type = "artifact"
source_family = "Test"
target_type = "file"
edge_type = "references"

[rules.match_signals]
confidence = "direct"
"#;
        let pack = parse_rule_pack(toml_str).unwrap();
        assert_eq!(pack.rules[0].match_signals.confidence, "direct");
        assert!(pack.rules[0].match_signals.caveats.is_none());
    }

    #[test]
    fn parse_all_operators() {
        let toml_str = r#"
[manifest]
name = "ops"
version = "1.0.0"
author = "test"
description = "All operators"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "r-equals"
name = "Equals"
description = "Equals op"
source_type = "artifact"
source_family = "Test"
target_type = "file"
edge_type = "references"

[[rules.conditions]]
field = "f"
operator = "equals"
target_field = "tf"

[rules.match_signals]
confidence = "heuristic"

[[rules]]
id = "r-regex"
name = "Regex"
description = "Regex op"
source_type = "artifact"
source_family = "Test"
target_type = "file"
edge_type = "references"

[[rules.conditions]]
field = "f"
operator = "regex"
target_field = "tf"

[rules.match_signals]
confidence = "heuristic"
"#;
        let pack = parse_rule_pack(toml_str).unwrap();
        assert_eq!(pack.rules[0].conditions[0].operator, Operator::Equals);
        assert_eq!(pack.rules[1].conditions[0].operator, Operator::Regex);
    }

    #[test]
    fn parse_all_edge_types() {
        for (edge_str, expected) in [
            ("contains", EdgeType::Contains),
            ("references", EdgeType::References),
            ("correlates_with", EdgeType::CorrelatesWith),
            ("derives_from", EdgeType::DerivesFrom),
            ("precedes", EdgeType::Precedes),
            ("cites", EdgeType::Cites),
            ("annotates", EdgeType::Annotates),
        ] {
            let toml_str = format!(
                r#"
[manifest]
name = "edge-test"
version = "1.0.0"
author = "test"
description = "Test"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "r-{edge_str}"
name = "Test"
description = "Test"
source_type = "artifact"
source_family = "Test"
target_type = "file"
edge_type = "{edge_str}"

[rules.match_signals]
confidence = "heuristic"
"#
            );
            let pack = parse_rule_pack(&toml_str).unwrap();
            assert_eq!(pack.rules[0].edge_type, expected, "edge_type {edge_str}");
        }
    }
}
