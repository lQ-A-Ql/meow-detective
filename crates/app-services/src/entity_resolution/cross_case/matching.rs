use std::collections::{BTreeMap, BTreeSet};

use super::model::{CrossCaseMatch, LoadedEntity, MatchStrategy};
use super::normalization::{match_id, secondary_normalize};

type Claimed = BTreeSet<(usize, String)>;

pub(super) fn match_entities(entities: &[LoadedEntity]) -> Vec<CrossCaseMatch> {
    if entities.is_empty() {
        return Vec::new();
    }
    let mut claimed = Claimed::new();
    let mut matches = exact_matches(entities, &mut claimed);
    matches.extend(normalized_matches(entities, &mut claimed));
    matches.extend(fuzzy_matches(entities, &claimed));
    matches.sort_by(|left, right| {
        left.match_strategy
            .cmp(&right.match_strategy)
            .then_with(|| left.entity_type.cmp(&right.entity_type))
            .then_with(|| left.id.cmp(&right.id))
    });
    matches
}

fn exact_matches(entities: &[LoadedEntity], claimed: &mut Claimed) -> Vec<CrossCaseMatch> {
    let groups = group_indices(entities, |entity| {
        (entity.canonical_value.clone(), entity.entity_type.clone())
    });
    project_groups(entities, groups, claimed, MatchStrategy::Exact, 0.95)
}

fn normalized_matches(entities: &[LoadedEntity], claimed: &mut Claimed) -> Vec<CrossCaseMatch> {
    let remaining = unclaimed_indices(entities, claimed);
    let groups = group_selected(&remaining, entities, |entity| {
        (
            secondary_normalize(&entity.canonical_value, &entity.entity_type),
            entity.entity_type.clone(),
        )
    });
    project_groups(entities, groups, claimed, MatchStrategy::Normalized, 0.85)
}

fn fuzzy_matches(entities: &[LoadedEntity], claimed: &Claimed) -> Vec<CrossCaseMatch> {
    let remaining = unclaimed_indices(entities, claimed);
    let groups = group_selected(&remaining, entities, |entity| {
        ("fuzzy".to_string(), entity.entity_type.clone())
    });
    project_groups_read_only(entities, groups, MatchStrategy::Fuzzy, 0.50)
}

fn project_groups(
    entities: &[LoadedEntity],
    groups: BTreeMap<(String, String), Vec<usize>>,
    claimed: &mut Claimed,
    strategy: MatchStrategy,
    confidence: f64,
) -> Vec<CrossCaseMatch> {
    for indices in groups
        .values()
        .filter(|indices| spans_databases(entities, indices))
    {
        for &index in indices {
            let entity = &entities[index];
            claimed.insert((entity.database_index, entity.entity_id.clone()));
        }
    }
    project_groups_read_only(entities, groups, strategy, confidence)
}

fn project_groups_read_only(
    entities: &[LoadedEntity],
    groups: BTreeMap<(String, String), Vec<usize>>,
    strategy: MatchStrategy,
    confidence: f64,
) -> Vec<CrossCaseMatch> {
    groups
        .into_iter()
        .filter(|(_, indices)| spans_databases(entities, indices))
        .map(|((seed, entity_type), indices)| {
            build_match(
                entities,
                indices,
                entity_type,
                seed,
                strategy.clone(),
                confidence,
            )
        })
        .collect()
}

fn build_match(
    all: &[LoadedEntity],
    indices: Vec<usize>,
    entity_type: String,
    seed: String,
    strategy: MatchStrategy,
    confidence: f64,
) -> CrossCaseMatch {
    let mut entities: Vec<(String, String, String)> = indices
        .into_iter()
        .map(|index| {
            let entity = &all[index];
            (
                entity.case_id.clone(),
                entity.entity_id.clone(),
                entity.canonical_value.clone(),
            )
        })
        .collect();
    entities.sort();
    let id_seed = match &strategy {
        MatchStrategy::Normalized => entities
            .first()
            .map(|entity| entity.2.as_str())
            .unwrap_or(seed.as_str()),
        MatchStrategy::Exact | MatchStrategy::Fuzzy => seed.as_str(),
    };
    CrossCaseMatch {
        id: match_id(&entity_type, id_seed, &strategy),
        entities,
        entity_type,
        confidence,
        match_strategy: strategy,
    }
}

fn group_indices<F>(entities: &[LoadedEntity], key: F) -> BTreeMap<(String, String), Vec<usize>>
where
    F: Fn(&LoadedEntity) -> (String, String),
{
    group_selected(&(0..entities.len()).collect::<Vec<_>>(), entities, key)
}

fn group_selected<F>(
    indices: &[usize],
    entities: &[LoadedEntity],
    key: F,
) -> BTreeMap<(String, String), Vec<usize>>
where
    F: Fn(&LoadedEntity) -> (String, String),
{
    let mut groups = BTreeMap::new();
    for &index in indices {
        groups
            .entry(key(&entities[index]))
            .or_insert_with(Vec::new)
            .push(index);
    }
    groups
}

fn unclaimed_indices(entities: &[LoadedEntity], claimed: &Claimed) -> Vec<usize> {
    (0..entities.len())
        .filter(|&index| {
            let entity = &entities[index];
            !claimed.contains(&(entity.database_index, entity.entity_id.clone()))
        })
        .collect()
}

fn spans_databases(entities: &[LoadedEntity], indices: &[usize]) -> bool {
    indices
        .iter()
        .map(|&index| entities[index].database_index)
        .collect::<BTreeSet<_>>()
        .len()
        >= 2
}
