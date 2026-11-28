use super::StructureAnalysisError;
use domain::DataSource;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn select_sources<'a>(
    sources: &'a [DataSource],
    requested: &[String],
) -> Result<Vec<&'a DataSource>, StructureAnalysisError> {
    if requested.is_empty() {
        return Ok(sources.iter().collect());
    }
    let by_id = sources
        .iter()
        .map(|source| (source.id.0.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    requested
        .iter()
        .map(|source_id| {
            if !seen.insert(source_id.as_str()) {
                return Err(StructureAnalysisError::MissingSource(source_id.clone()));
            }
            by_id
                .get(source_id.as_str())
                .copied()
                .ok_or_else(|| StructureAnalysisError::MissingSource(source_id.clone()))
        })
        .collect()
}
