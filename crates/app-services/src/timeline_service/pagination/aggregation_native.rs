use super::ClusterRow;
use persistence_fjall::{ScanDirection, TimelineRecordStore};
use rusqlite::Connection;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};
use transport::dto::{TimelineAggregatedDto, TimelineClusterDto, TimelineStripeDto};

use super::TimelineServiceError;

struct RankedCluster(ClusterRow);

impl PartialEq for RankedCluster {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for RankedCluster {}

impl PartialOrd for RankedCluster {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedCluster {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .count
            .cmp(&self.0.count)
            .then_with(|| self.0.event_type.cmp(&other.0.event_type))
            .then_with(|| self.0.description.cmp(&other.0.description))
    }
}

pub(super) fn query(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<TimelineAggregatedDto, TimelineServiceError> {
    let source = persistence_sqlite::source_fjall::require_source_store(conn)
        .map_err(TimelineServiceError::Db)?;
    let source_id = persistence_sqlite::source_fjall::source_id(conn)?
        .ok_or_else(|| TimelineServiceError::Other("source identity is missing".into()))?;
    let repository = TimelineRecordStore::from_source_store((*source).clone());
    let snapshot = repository.read_snapshot();
    let (clusters, totals) = load_clusters(
        &repository,
        &snapshot,
        &source_id,
        page_capacity(offset, limit),
    )?;
    Ok(build_aggregated(clusters, totals, offset, limit))
}

fn load_clusters(
    repository: &TimelineRecordStore,
    snapshot: &persistence_fjall::SourceReadSnapshot,
    source_id: &str,
    page_capacity: usize,
) -> Result<(Vec<ClusterRow>, HashMap<String, u64>), TimelineServiceError> {
    let mut cursor = None;
    let mut retained = BinaryHeap::<RankedCluster>::with_capacity(page_capacity);
    let mut totals = HashMap::<String, u64>::new();
    let mut current = None;
    loop {
        let page = repository
            .list_by_cluster(
                snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .map_err(|error| TimelineServiceError::Other(error.to_string()))?;
        for record in page.records {
            update_cluster(
                &mut current,
                &record,
                page_capacity,
                &mut retained,
                &mut totals,
            );
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    if let Some(cluster) = current {
        retain_cluster(cluster, page_capacity, &mut retained, &mut totals);
    }
    let mut rows = retained
        .into_iter()
        .map(|ranked| ranked.0)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.event_type.cmp(&right.event_type))
            .then_with(|| left.description.cmp(&right.description))
    });
    Ok((rows, totals))
}

fn update_cluster(
    current: &mut Option<ClusterRow>,
    record: &persistence_fjall::TimelineRecord,
    page_capacity: usize,
    retained: &mut BinaryHeap<RankedCluster>,
    totals: &mut HashMap<String, u64>,
) {
    let starts_new_cluster = current.as_ref().is_some_and(|row| {
        row.event_type != record.event_type || row.description != record.description
    });
    if starts_new_cluster {
        if let Some(row) = current.take() {
            retain_cluster(row, page_capacity, retained, totals);
        }
    }
    let row = current.get_or_insert_with(|| ClusterRow {
        event_type: record.event_type.clone(),
        description: record.description.clone(),
        count: 0,
        first_ts: record.timestamp.to_rfc3339(),
        last_ts: record.timestamp.to_rfc3339(),
        sample_ids: String::new(),
    });
    row.count = row.count.saturating_add(1);
    let timestamp = record.timestamp.to_rfc3339();
    if timestamp < row.first_ts {
        row.first_ts = timestamp.clone();
    }
    if timestamp > row.last_ts {
        row.last_ts = timestamp;
    }
    if row
        .sample_ids
        .split(',')
        .filter(|value| !value.is_empty())
        .count()
        < 5
    {
        if !row.sample_ids.is_empty() {
            row.sample_ids.push(',');
        }
        row.sample_ids.push_str(&record.id);
    }
}

fn retain_cluster(
    cluster: ClusterRow,
    page_capacity: usize,
    retained: &mut BinaryHeap<RankedCluster>,
    totals: &mut HashMap<String, u64>,
) {
    *totals.entry(cluster.event_type.clone()).or_default() += cluster.count;
    if page_capacity == 0 {
        return;
    }
    let candidate = RankedCluster(cluster);
    if retained.len() < page_capacity {
        retained.push(candidate);
    } else if retained.peek().is_some_and(|worst| candidate < *worst) {
        retained.pop();
        retained.push(candidate);
    }
}

fn build_aggregated(
    clusters: Vec<ClusterRow>,
    totals: HashMap<String, u64>,
    offset: u64,
    limit: u32,
) -> TimelineAggregatedDto {
    let rows = clusters
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect::<Vec<_>>();
    let event_types = rows
        .iter()
        .map(|row| row.event_type.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut stripes = event_types
        .iter()
        .map(|event_type| {
            (
                event_type.clone(),
                TimelineStripeDto {
                    clusters: Vec::new(),
                    total_events: *totals.get(event_type).unwrap_or(&0),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for row in rows {
        stripes
            .entry(row.event_type.clone())
            .or_insert_with(|| TimelineStripeDto {
                clusters: Vec::new(),
                total_events: 0,
            })
            .clusters
            .push(TimelineClusterDto {
                event_type: row.event_type,
                description: row.description,
                count: row.count,
                first_ts: row.first_ts,
                last_ts: row.last_ts,
                sample_event_ids: row
                    .sample_ids
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect(),
            });
    }
    for event_type in event_types {
        stripes
            .entry(event_type)
            .or_insert_with(|| TimelineStripeDto {
                clusters: Vec::new(),
                total_events: 0,
            });
    }
    TimelineAggregatedDto {
        stripes_by_type: stripes,
    }
}

fn page_capacity(offset: u64, limit: u32) -> usize {
    usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .saturating_add(limit as usize)
}
