use super::super::error::ParallelEnumError;
use super::path_reconstruction::PathMap;
use persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(in crate::parallel_enum) struct PathMaterializationStats {
    pub compute_elapsed: Duration,
    pub sql_elapsed: Duration,
    pub updated_main_rows: u64,
    pub materialized_rows: u64,
    pub staging_logical_bytes: u64,
}

pub(in crate::parallel_enum) fn update_mft_staging_paths_via_sqlite(
    conn: &Connection,
    _data_source_id: &str,
    _partition_index: usize,
    path_map: &PathMap,
    deleted_records: &HashSet<u64>,
    cancel: &AtomicBool,
) -> Result<PathMaterializationStats, ParallelEnumError> {
    let compute_started = Instant::now();
    let mut resolved = HashMap::with_capacity(path_map.len());
    let mut records: Vec<_> = path_map.keys().copied().collect();
    records.sort_unstable();
    let mut budget = PathBudget {
        used: 0,
        maximum: 256 * 1024 * 1024,
    };
    for record in &records {
        resolve_mft_path(
            *record,
            path_map,
            deleted_records,
            &mut resolved,
            &mut budget,
            cancel,
        )?;
    }
    let compute_elapsed = compute_started.elapsed();
    let sql_started = Instant::now();
    let materialized_rows = NtfsStagingRepo::write_paths(conn, |statement| {
        let mut rows = 0u64;
        for record in records {
            check_cancel(cancel)?;
            let (parent, name, _) = &path_map[&record];
            let parent = if record == 5 {
                None
            } else {
                parent
                    .filter(|p| path_map.contains_key(p))
                    .or_else(|| path_map.contains_key(&5).then_some(5))
            };
            let path = &resolved[&record];
            let status = if record == 5 {
                0
            } else if path.starts_with("/$DeletedOrphans/") {
                2
            } else if path.starts_with("/Unresolved/") {
                3
            } else {
                1
            };
            rows += statement.execute(params![record, path, name, parent, status])? as u64;
        }
        Ok::<_, ParallelEnumError>(rows)
    })?;
    let (pages,size):(u64,u64) = conn.query_row("SELECT (SELECT page_count FROM pragma_page_count),(SELECT page_size FROM pragma_page_size)",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    Ok(PathMaterializationStats {
        compute_elapsed,
        sql_elapsed: sql_started.elapsed(),
        updated_main_rows: 0,
        materialized_rows,
        staging_logical_bytes: pages.saturating_mul(size),
    })
}

struct PathBudget {
    used: usize,
    maximum: usize,
}
impl PathBudget {
    fn store(
        &mut self,
        cache: &mut HashMap<u64, String>,
        id: u64,
        path: String,
    ) -> Result<(), ParallelEnumError> {
        if path.len() > 256 * 1024 || self.used.saturating_add(path.capacity()) > self.maximum {
            return Err(ParallelEnumError::MftResourceLimit("resolved path bytes"));
        }
        self.used += path.capacity();
        cache.insert(id, path);
        Ok(())
    }
}

// Preserve the old iterative raw-path values. Partition prefixes are still
// applied by the existing final catalog path/merge consumers, never here.
fn resolve_mft_path(
    record: u64,
    path_map: &PathMap,
    deleted_records: &HashSet<u64>,
    resolved: &mut HashMap<u64, String>,
    budget: &mut PathBudget,
    cancel: &AtomicBool,
) -> Result<(), ParallelEnumError> {
    check_cancel(cancel)?;
    if resolved.contains_key(&record) {
        return Ok(());
    }
    let mut chain = Vec::new();
    let mut current = record;
    let mut seen = HashSet::from([current]);
    let base = loop {
        check_cancel(cancel)?;
        if chain.len() >= 65_536 {
            return Err(ParallelEnumError::MftResourceLimit("path ancestor chain"));
        }
        if let Some(path) = resolved.get(&current) {
            break path.clone();
        }
        match path_map.get(&current) {
            Some((Some(parent), _, _))
                if parent != &current && path_map.contains_key(parent) && seen.insert(*parent) =>
            {
                chain.push(current);
                current = *parent;
            }
            Some(_) if current == 5 => {
                budget.store(resolved, current, String::new())?;
                break String::new();
            }
            Some((_, name, _)) => {
                let path = if deleted_records.contains(&current) {
                    format!("/$DeletedOrphans/{current}-{name}")
                } else {
                    format!("/Unresolved/{name}")
                };
                budget.store(resolved, current, path.clone())?;
                break path;
            }
            None => {
                let path = format!("/Unresolved/{current}");
                budget.store(resolved, current, path.clone())?;
                break path;
            }
        }
    };
    let mut path = base;
    for id in chain.into_iter().rev() {
        check_cancel(cancel)?;
        let name = &path_map[&id].1;
        path = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        budget.store(resolved, id, path.clone())?;
    }
    Ok(())
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), ParallelEnumError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ParallelEnumError::Cancelled)
    } else {
        Ok(())
    }
}
