mod loading;
mod matching;
mod model;
mod normalization;

pub use model::{CrossCaseMatch, MatchStrategy};

use std::path::PathBuf;

use super::EntityResolutionError;

/// Matches resolved entities across two or more case databases.
pub struct CrossCaseEntityMatcher;

impl CrossCaseEntityMatcher {
    pub fn match_entities_across_cases(
        db_paths: &[PathBuf],
    ) -> Result<Vec<CrossCaseMatch>, EntityResolutionError> {
        if db_paths.len() < 2 {
            return Err(EntityResolutionError::InvalidInput(
                "cross_case: at least 2 database paths are required".into(),
            ));
        }
        let entities = loading::load_entities(db_paths)?;
        Ok(matching::match_entities(&entities))
    }
}
