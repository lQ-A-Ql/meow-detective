use serde::{Deserialize, Serialize};

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
    fn from(node_type: NodeType) -> Self {
        match node_type {
            NodeType::File => Self::File,
            NodeType::Artifact => Self::Artifact,
            NodeType::TimelineEvent => Self::TimelineEvent,
            NodeType::Entity => Self::Entity,
            NodeType::Lead => Self::Lead,
            NodeType::NotebookEntry => Self::NotebookEntry,
        }
    }
}

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
    fn from(edge_type: EdgeType) -> Self {
        match edge_type {
            EdgeType::Contains => Self::Contains,
            EdgeType::References => Self::References,
            EdgeType::CorrelatesWith => Self::CorrelatesWith,
            EdgeType::DerivesFrom => Self::DerivesFrom,
            EdgeType::Precedes => Self::Precedes,
            EdgeType::Cites => Self::Cites,
            EdgeType::Annotates => Self::Annotates,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Equals,
    Contains,
    Regex,
    PathEquals,
    FilenameEquals,
    TemporalProximity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub scope: Vec<String>,
    pub min_product_version: String,
    #[serde(default)]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSignals {
    pub confidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caveats: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPredicate {
    pub field: String,
    pub operator: Operator,
    pub target_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_type: NodeType,
    pub source_family: String,
    pub target_type: NodeType,
    pub edge_type: EdgeType,
    #[serde(default)]
    pub conditions: Vec<FieldPredicate>,
    pub match_signals: MatchSignals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePack {
    pub manifest: RulePackManifest,
    #[serde(default)]
    pub rules: Vec<RuleDefinition>,
}
