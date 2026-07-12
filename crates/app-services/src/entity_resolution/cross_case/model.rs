/// How entities from different cases were matched.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchStrategy {
    Exact,
    Normalized,
    Fuzzy,
}

impl MatchStrategy {
    pub(super) fn tag(&self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Normalized => "norm",
            Self::Fuzzy => "fuzzy",
        }
    }
}

/// A deterministic group of matching resolved entities.
#[derive(Debug, Clone)]
pub struct CrossCaseMatch {
    pub id: String,
    pub entities: Vec<(String, String, String)>,
    pub entity_type: String,
    pub confidence: f64,
    pub match_strategy: MatchStrategy,
}

#[derive(Debug, Clone)]
pub(super) struct LoadedEntity {
    pub case_id: String,
    pub entity_id: String,
    pub canonical_value: String,
    pub entity_type: String,
    pub database_index: usize,
}
