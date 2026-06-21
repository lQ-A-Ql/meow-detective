//! Post-import tier state machine.
//!
//! Tracks which tier the post-import pipeline is currently processing and
//! stores results from each completed tier for partial inspection.
//!
//! The pipeline advances: Catalog → ExtractArtifacts → CorrelateAndIndex → Done.

use std::collections::HashMap;
use std::fmt;

/// Ordered tiers in the post-import pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    /// File enumeration (MFT / directory walk) — files are counted and queued.
    Catalog,
    /// Artifact extraction and timeline projection.
    ExtractArtifacts,
    /// Correlation rule execution and search-index finalisation.
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

/// Per-tier result available for partial inspection after the tier completes.
#[derive(Debug, Clone, Default)]
pub struct TierResult {
    pub completed: bool,
    pub stats: String,
    pub warnings: Vec<String>,
}

/// Tracks which tier the post-import pipeline is currently in and stores
/// results from each completed tier for partial inspection.
#[derive(Debug, Clone)]
pub struct TierStateMachine {
    /// The tier currently being processed, or None before start / after all done.
    pub current_tier: Option<Tier>,
    /// Results from each completed tier.
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

/// Advance to the next tier in the pipeline.
///
/// Before advancing, the current tier (if any) is marked as completed in the
/// results map.  Returns the new current tier, or None if all tiers are done.
///
/// Transition: None → Catalog → ExtractArtifacts → CorrelateAndIndex → None.
pub fn advance_tier(state: &mut TierStateMachine) -> Option<Tier> {
    // Once all tiers are complete, stay done.
    if state.current_tier.is_none()
        && state
            .tier_results
            .get(&Tier::CorrelateAndIndex)
            .is_some_and(|r| r.completed)
    {
        return None;
    }

    // Mark the current tier as completed before moving on.
    if let Some(current) = state.current_tier {
        let result = state.tier_results.entry(current).or_default();
        result.completed = true;
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
mod tests {
    use super::*;

    #[test]
    fn tier_advances_catalog_to_extract() {
        let mut sm = TierStateMachine::new();
        assert_eq!(sm.current_tier, None);

        let tier = advance_tier(&mut sm);
        assert_eq!(tier, Some(Tier::Catalog));
        assert_eq!(sm.current_tier, Some(Tier::Catalog));
        // Catalog is not yet completed — it's the active tier.
        assert!(!sm
            .tier_results
            .get(&Tier::Catalog)
            .is_some_and(|r| r.completed));

        // After MFT enumeration, advance again.
        let tier = advance_tier(&mut sm);
        assert_eq!(tier, Some(Tier::ExtractArtifacts));
        assert_eq!(sm.current_tier, Some(Tier::ExtractArtifacts));
        // Catalog should now be marked completed.
        assert!(sm
            .tier_results
            .get(&Tier::Catalog)
            .is_some_and(|r| r.completed));
    }

    #[test]
    fn partial_results_accessible_at_catalog_tier() {
        let mut sm = TierStateMachine::new();
        // Start Catalog.
        advance_tier(&mut sm);
        // Add some partial stats before the tier completes.
        sm.tier_results.entry(Tier::Catalog).or_default().stats = "files_queued=42".to_string();

        // Complete Catalog, move to Extract.
        advance_tier(&mut sm);

        let catalog_result = sm.tier_results.get(&Tier::Catalog).unwrap();
        assert!(catalog_result.completed);
        assert_eq!(catalog_result.stats, "files_queued=42");
        // Extract tier is not yet complete.
        assert!(!sm
            .tier_results
            .get(&Tier::ExtractArtifacts)
            .is_some_and(|r| r.completed));
    }

    #[test]
    fn all_tiers_complete() {
        let mut sm = TierStateMachine::new();

        // Advance through all three tiers.
        assert_eq!(advance_tier(&mut sm), Some(Tier::Catalog));
        assert_eq!(advance_tier(&mut sm), Some(Tier::ExtractArtifacts));
        assert_eq!(advance_tier(&mut sm), Some(Tier::CorrelateAndIndex));

        // All tiers should be active / completed.
        assert!(sm
            .tier_results
            .get(&Tier::Catalog)
            .is_some_and(|r| r.completed));
        assert!(sm
            .tier_results
            .get(&Tier::ExtractArtifacts)
            .is_some_and(|r| r.completed));
        assert_eq!(sm.current_tier, Some(Tier::CorrelateAndIndex));

        // Final advance — all done.
        assert_eq!(advance_tier(&mut sm), None);
        assert!(sm
            .tier_results
            .get(&Tier::CorrelateAndIndex)
            .is_some_and(|r| r.completed));
        assert_eq!(sm.current_tier, None);

        // Another call stays done.
        assert_eq!(advance_tier(&mut sm), None);
    }
}
