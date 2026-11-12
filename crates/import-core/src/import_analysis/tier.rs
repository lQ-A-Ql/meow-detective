//! Post-import tier state machine.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    Catalog,
    ExtractArtifacts,
    CorrelateAndIndex,
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tier::Catalog => write!(f, "catalog"),
            Tier::ExtractArtifacts => write!(f, "extract-artifacts"),
            Tier::CorrelateAndIndex => write!(f, "correlate-and-index"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TierResult {
    pub completed: bool,
    pub stats: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TierStateMachine {
    pub current_tier: Option<Tier>,
    pub tier_results: HashMap<Tier, TierResult>,
}

impl TierStateMachine {
    pub fn new() -> Self {
        Self {
            current_tier: None,
            tier_results: HashMap::new(),
        }
    }
}

impl Default for TierStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

pub fn advance_tier(state: &mut TierStateMachine) -> Option<Tier> {
    if state.current_tier.is_none()
        && state
            .tier_results
            .get(&Tier::CorrelateAndIndex)
            .is_some_and(|result| result.completed)
    {
        return None;
    }

    if let Some(current) = state.current_tier {
        state.tier_results.entry(current).or_default().completed = true;
    }

    let next = match state.current_tier {
        None => Tier::Catalog,
        Some(Tier::Catalog) => Tier::ExtractArtifacts,
        Some(Tier::ExtractArtifacts) => Tier::CorrelateAndIndex,
        Some(Tier::CorrelateAndIndex) => {
            state.current_tier = None;
            return None;
        }
    };

    state.current_tier = Some(next);
    Some(next)
}

#[cfg(test)]
#[path = "../../tests/unit/import_analysis/tier.rs"]
mod tests;
