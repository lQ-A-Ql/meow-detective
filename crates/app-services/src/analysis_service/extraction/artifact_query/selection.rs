use super::{AnalysisArtifactRow, AnalysisServiceError};
use chrono::{DateTime, Utc};
use domain::Artifact;
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use serde_json::Value;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashSet},
};

#[derive(Clone, Copy)]
pub(super) enum Order {
    Created,
    Event,
    Id,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Key(Reverse<Option<DateTime<Utc>>>, i128, String);

struct Ranked {
    key: Key,
    artifact: Artifact,
}

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for Ranked {}
impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

pub(super) fn visit(
    conn: &Connection,
    families: Option<&[&str]>,
    mut consume: impl FnMut(Artifact) -> persistence_sqlite::DbResult<()>,
) -> Result<(), AnalysisServiceError> {
    let repository = ArtifactRepo::new(conn);
    if let Some(families) = families {
        let mut visited = HashSet::new();
        for family in families {
            if visited.insert(*family) {
                repository.visit_by_family(Some(family), &mut consume)?;
            }
        }
    } else {
        repository.visit_by_family(None, &mut consume)?;
    }
    Ok(())
}

pub(super) fn count(
    conn: &Connection,
    families: Option<&[&str]>,
    filter: impl Fn(&Artifact) -> bool,
) -> Result<u64, AnalysisServiceError> {
    let mut total = 0u64;
    visit(conn, families, |artifact| {
        if filter(&artifact) {
            total += 1;
        }
        Ok(())
    })?;
    Ok(total)
}

pub(super) fn select(
    conn: &Connection,
    families: &[&str],
    filter: impl Fn(&Artifact) -> bool,
    order: Order,
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let window = offset
        .checked_add(u64::from(limit))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            AnalysisServiceError::InvalidInput("artifact page range exceeds platform limits".into())
        })?;
    let mut ranked = BinaryHeap::new();
    visit(conn, Some(families), |artifact| {
        if filter(&artifact) {
            let candidate = Ranked {
                key: key(&artifact, order),
                artifact,
            };
            if ranked.len() < window {
                ranked.push(candidate);
            } else if ranked.peek().is_some_and(|worst| candidate.key < worst.key) {
                ranked.pop();
                ranked.push(candidate);
            }
        }
        Ok(())
    })?;
    Ok(ranked
        .into_sorted_vec()
        .into_iter()
        .skip(usize::try_from(offset).map_err(|_| {
            AnalysisServiceError::InvalidInput("artifact offset exceeds platform limits".into())
        })?)
        .map(|item| row(item.artifact))
        .collect())
}

fn key(artifact: &Artifact, order: Order) -> Key {
    let (time, sequence) = match order {
        Order::Created => (Some(artifact.created_at), 0),
        Order::Event => (
            artifact
                .attrs
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc)),
            -i128::from(integer(artifact.attrs.get("recordId"))),
        ),
        Order::Id => (None, 0),
    };
    Key(Reverse(time), sequence, artifact.id.0.clone())
}

pub(super) fn integer(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .unwrap_or(0)
}

pub(super) fn host_config(artifact: &Artifact, kind: &str) -> bool {
    artifact.attrs.get("configKind").and_then(Value::as_str) == Some(kind)
        && artifact
            .attrs
            .get("overlayContext")
            .is_none_or(Value::is_null)
}

pub(super) fn row(artifact: Artifact) -> AnalysisArtifactRow {
    AnalysisArtifactRow {
        id: artifact.id.0,
        source_object_id: artifact.source_object_id.map(|id| id.0),
        extractor_id: artifact.extractor_id,
        created_at: artifact.created_at.to_rfc3339(),
        attrs: artifact.attrs,
    }
}
