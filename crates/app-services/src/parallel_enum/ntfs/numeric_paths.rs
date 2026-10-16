use super::path_reconstruction::PathMap;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub(in crate::parallel_enum) struct PathMaterializationStats {
    pub compute_elapsed: Duration,
    pub sql_elapsed: Duration,
    pub updated_main_rows: u64,
}

pub(in crate::parallel_enum) fn update_mft_staging_paths_via_sqlite(
    conn: &Connection,
    _data_source_id: &str,
    partition_index: usize,
    path_map: &PathMap,
    deleted_records: &HashSet<u64>,
) -> rusqlite::Result<PathMaterializationStats> {
    let compute_started = Instant::now();
    let mut resolved = HashMap::with_capacity(path_map.len());
    let mut records: Vec<_> = path_map.keys().copied().collect();
    records.sort_unstable();
    for record in &records {
        resolve_mft_path(
            record,
            path_map,
            deleted_records,
            &mut resolved,
            partition_index,
        );
    }
    let compute_elapsed = compute_started.elapsed();
    let sql_started = Instant::now();
    let mut updated_main_rows = 0;
    let mut statement = conn.prepare_cached(
        "UPDATE ntfs_entries SET path=?1, name=?2, parent_record_num=?3 WHERE record_num=?4",
    )?;
    for record in records {
        let (parent, name, _) = &path_map[&record];
        let parent = if record == 5 {
            None
        } else {
            parent
                .filter(|p| path_map.contains_key(p))
                .or_else(|| path_map.contains_key(&5).then_some(5))
        };
        updated_main_rows +=
            statement.execute(params![resolved[&record], name, parent, record])? as u64;
    }
    Ok(PathMaterializationStats {
        compute_elapsed,
        sql_elapsed: sql_started.elapsed(),
        updated_main_rows,
    })
}

fn resolve_mft_path(
    record: &u64,
    path_map: &PathMap,
    deleted_records: &HashSet<u64>,
    resolved: &mut HashMap<u64, String>,
    partition_index: usize,
) -> String {
    if let Some(path) = resolved.get(record) {
        return path.clone();
    }
    let mut chain = Vec::new();
    let mut current = *record;
    let mut seen = HashSet::from([current]);
    let base = loop {
        if let Some(path) = resolved.get(&current) {
            break path.clone();
        }
        match path_map.get(&current) {
            Some((Some(parent), name, _))
                if parent != &current && path_map.contains_key(parent) && seen.insert(*parent) =>
            {
                chain.push((current, name.clone()));
                current = *parent;
            }
            Some((_, _, _)) if current == 5 => {
                resolved.insert(current, String::new());
                break String::new();
            }
            Some((_, name, _)) if deleted_records.contains(&current) => {
                let path = format!("/$DeletedOrphans/{current}-{name}");
                resolved.insert(current, path.clone());
                break path;
            }
            Some((_, name, _)) => {
                let path = format!("/Unresolved/{name}");
                resolved.insert(current, path.clone());
                break path;
            }
            None => {
                let path = format!("/Unresolved/{current}");
                resolved.insert(current, path.clone());
                break path;
            }
        }
    };
    let mut path = base;
    for (id, name) in chain.into_iter().rev() {
        path = if path.is_empty() {
            name
        } else {
            format!("{path}/{name}")
        };
        resolved.insert(id, path.clone());
    }
    let path = resolved.get(record).cloned().unwrap_or(path);
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        format!("[P{partition_index}]")
    } else {
        format!("[P{partition_index}]/{trimmed}")
    }
}
