//! Handle-free, byte-bounded E01 plans; resource waits never hold the cache lock.
use crate::resource_budget::MIB;
use image_e01::{E01OpenPlan, E01Reader};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex, Weak,
    },
};

pub(crate) const E01_READER_CACHE_PER_CASE_MAX_SIZE: usize = 16;
const CASE_BYTES: u64 = 258 * MIB;
const GLOBAL_BYTES: u64 = 258 * MIB;
const MAX_CASES: usize = 32;
struct PlanEntry {
    plan: E01OpenPlan,
    used: u64,
}
struct Bucket {
    plans: HashMap<PathBuf, PlanEntry>,
    used: u64,
    cancel: Arc<AtomicBool>,
}
pub(crate) struct E01ReaderCacheRegistry {
    buckets: HashMap<String, Bucket>,
    flags: HashMap<String, Weak<AtomicBool>>,
    clock: u64,
    factories: HashMap<(String, PathBuf), Vec<Weak<crate::source_runtime::resources::SharedPlan>>>,
}
impl E01ReaderCacheRegistry {
    fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            flags: HashMap::new(),
            clock: 0,
            factories: HashMap::new(),
        }
    }
    fn tick(&mut self) -> u64 {
        self.clock = self.clock.wrapping_add(1);
        self.clock
    }
    fn bucket(&mut self, case: &str) -> &mut Bucket {
        let used = self.tick();
        if !self.buckets.contains_key(case) {
            self.flags.retain(|_, v| v.strong_count() > 0);
            let cancel = self
                .flags
                .get(case)
                .and_then(Weak::upgrade)
                .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            self.flags.insert(case.into(), Arc::downgrade(&cancel));
            if self.buckets.len() >= MAX_CASES {
                if let Some(old) = self
                    .buckets
                    .iter()
                    .min_by_key(|(_, b)| b.used)
                    .map(|(k, _)| k.clone())
                {
                    self.buckets.remove(&old);
                }
            }
            self.buckets.insert(
                case.into(),
                Bucket {
                    plans: HashMap::new(),
                    used,
                    cancel,
                },
            );
        }
        let bucket = self.buckets.get_mut(case).expect("bucket inserted");
        bucket.used = used;
        bucket
    }
    fn lookup(&mut self, path: &Path, case: &str) -> (Option<E01OpenPlan>, Arc<AtomicBool>) {
        let shared = self
            .factories
            .get_mut(&(case.into(), path.into()))
            .and_then(|plans| {
                plans.retain(|p| p.strong_count() > 0);
                let mut live: Vec<_> = plans.iter().filter_map(Weak::upgrade).collect();
                live.sort_by_key(|p| p.route.scope().is_some());
                live.pop()
            });
        let used = self.clock.wrapping_add(1);
        let bucket = self.bucket(case);
        if let Some(shared) = shared {
            use crate::source_runtime::resources::{CaseReaderAdmission, ReaderBudget};
            let admission = CaseReaderAdmission {
                budget: ReaderBudget {
                    bytes: shared.plan.reader_memory_bound(),
                    handles: shared.plan.segment_count() as u64,
                    route: shared.route.clone(),
                },
                cancel: bucket.cancel.clone(),
            };
            return (
                Some(
                    shared
                        .plan
                        .clone()
                        .with_reader_admission(Arc::new(admission)),
                ),
                bucket.cancel.clone(),
            );
        }
        let plan = bucket.plans.get_mut(path).map(|e| {
            e.used = used;
            e.plan.clone()
        });
        (plan, bucket.cancel.clone())
    }
    fn insert(
        &mut self,
        path: &Path,
        case: &str,
        plan: E01OpenPlan,
        cancel: &Arc<AtomicBool>,
    ) -> std::io::Result<()> {
        if cancel.load(Ordering::Acquire) {
            return Err(std::io::Error::other(
                crate::source_runtime::SourceInvalidated,
            ));
        }
        let used = self.clock.wrapping_add(1);
        let bucket = self.bucket(case);
        if !Arc::ptr_eq(cancel, &bucket.cancel) {
            return Err(std::io::Error::other(
                crate::source_runtime::SourceInvalidated,
            ));
        }
        if plan.metadata_bytes() > CASE_BYTES {
            return Ok(());
        }
        bucket.plans.insert(path.into(), PlanEntry { plan, used });
        while bucket.plans.len() > E01_READER_CACHE_PER_CASE_MAX_SIZE
            || bucket
                .plans
                .values()
                .map(|e| e.plan.metadata_bytes())
                .sum::<u64>()
                > CASE_BYTES
        {
            if let Some(old) = bucket
                .plans
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(p, _)| p.clone())
            {
                bucket.plans.remove(&old);
            } else {
                break;
            }
        }
        while self
            .buckets
            .values()
            .flat_map(|b| b.plans.values())
            .map(|e| e.plan.metadata_bytes())
            .sum::<u64>()
            > GLOBAL_BYTES
        {
            let old = self
                .buckets
                .iter()
                .flat_map(|(case, b)| {
                    b.plans
                        .iter()
                        .map(move |(p, e)| (case.clone(), p.clone(), e.used))
                })
                .min_by_key(|e| e.2);
            if let Some((case, path, _)) = old {
                self.buckets
                    .get_mut(&case)
                    .expect("existing bucket")
                    .plans
                    .remove(&path);
            } else {
                break;
            }
        }
        Ok(())
    }
    fn clear_case(&mut self, case: &str) {
        self.factories.retain(|(key, _), _| key != case);
        if let Some(flag) = self.flags.remove(case).and_then(|f| f.upgrade()) {
            flag.store(true, Ordering::Release);
        }
        self.buckets.remove(case);
    }
    fn clear(&mut self) {
        self.buckets.clear();
        self.flags.retain(|_, flag| flag.strong_count() > 0);
    }
}
pub(crate) static E01_READER_CACHE: LazyLock<Mutex<E01ReaderCacheRegistry>> =
    LazyLock::new(|| Mutex::new(E01ReaderCacheRegistry::new()));
fn registry() -> std::sync::MutexGuard<'static, E01ReaderCacheRegistry> {
    E01_READER_CACHE.lock().unwrap_or_else(|p| {
        let mut r = p.into_inner();
        r.clear();
        E01_READER_CACHE.clear_poison();
        r
    })
}
pub fn clear_e01_reader_cache() {
    registry().clear();
}
pub fn clear_e01_reader_cache_for_case(case_id: &str) {
    registry().clear_case(case_id);
}
pub(crate) fn evict_e01_plans_for_case(case_id: &str) {
    registry().buckets.remove(case_id);
}
pub(crate) fn register_factory_plan(
    case: &str,
    path: &Path,
    plan: &Arc<crate::source_runtime::resources::SharedPlan>,
) {
    let mut cache = registry();
    cache.factories.retain(|_, refs| {
        refs.retain(|r| r.strong_count() > 0);
        !refs.is_empty()
    });
    cache
        .factories
        .entry((case.into(), path.into()))
        .or_default()
        .push(Arc::downgrade(plan));
}
pub fn open_e01_reader_cached(path: &Path, case: &str) -> std::io::Result<E01Reader> {
    let path = path.canonicalize()?;
    let (cached, cancel) = registry().lookup(&path, case);
    if let Some(plan) = cached {
        return plan.open_reader();
    }
    let plan = crate::source_runtime::resources::open_e01_plan(
        &path,
        None,
        Some(cancel.clone()),
        Default::default(),
    )?;
    registry().insert(&path, case, plan.clone(), &cancel)?;
    plan.open_reader()
}
