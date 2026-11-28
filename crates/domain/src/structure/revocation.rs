use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, LazyLock, Mutex, Weak,
    },
};

type Key = (String, String, bool);
static EPOCHS: LazyLock<Mutex<HashMap<Key, Weak<AtomicU64>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub struct RevisionReadLease {
    epoch: Arc<AtomicU64>,
    observed: u64,
}

impl RevisionReadLease {
    pub fn acquire(case_id: &str, source_id: &str) -> Self {
        Self::acquire_kind(case_id, source_id, false)
    }

    pub fn acquire_generation(case_id: &str, source_id: &str) -> Self {
        Self::acquire_kind(case_id, source_id, true)
    }

    fn acquire_kind(case_id: &str, source_id: &str, generation: bool) -> Self {
        let mut epochs = EPOCHS.lock().unwrap_or_else(|poison| poison.into_inner());
        epochs.retain(|_, value| value.strong_count() > 0);
        let key = (case_id.into(), source_id.into(), generation);
        let epoch = epochs.get(&key).and_then(Weak::upgrade).unwrap_or_default();
        let observed = epoch.load(Ordering::Acquire);
        epochs.insert(key, Arc::downgrade(&epoch));
        Self { epoch, observed }
    }

    pub fn is_current(&self) -> bool {
        self.epoch.load(Ordering::Acquire) == self.observed
    }
}

pub fn revoke_source_revision(case_id: &str, source_id: &str) {
    revoke(|key| key.0 == case_id && key.1 == source_id);
}

pub fn revoke_source_generation(case_id: &str, source_id: &str) {
    revoke(|key| key.0 == case_id && key.1 == source_id && key.2);
}

pub fn revoke_case_revisions(case_id: &str) {
    revoke(|key| key.0 == case_id);
}

fn revoke(matches: impl Fn(&Key) -> bool) {
    let mut epochs = EPOCHS.lock().unwrap_or_else(|poison| poison.into_inner());
    epochs.retain(|key, value| {
        let Some(epoch) = value.upgrade() else {
            return false;
        };
        if matches(key) {
            epoch.fetch_add(1, Ordering::AcqRel);
        }
        true
    });
}
