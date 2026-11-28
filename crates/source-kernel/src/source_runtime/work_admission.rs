use std::{
    io,
    sync::{atomic::AtomicBool, Arc, Condvar, LazyLock, Mutex},
    time::Duration,
};

use super::range_view::check_active;
use crate::resource_budget::{ResourceError, ResourcePermit, ResourcePool, Resources};
use domain::structure::ReadPriority;

#[derive(Clone)]
pub struct WorkAdmission(Arc<Admission>);
struct Admission {
    pool: ResourcePool,
    waiting: Mutex<[u64; 3]>,
    changed: Condvar,
}
pub struct WorkPermit {
    permit: Option<ResourcePermit>,
    admission: WorkAdmission,
}
struct Waiting {
    admission: WorkAdmission,
    rank: usize,
}

impl WorkAdmission {
    pub fn new(pool: ResourcePool) -> Self {
        Self(Arc::new(Admission {
            pool,
            waiting: Mutex::new([0; 3]),
            changed: Condvar::new(),
        }))
    }

    pub fn pending(&self) -> io::Result<[u64; 3]> {
        self.0
            .waiting
            .lock()
            .map(|waiting| *waiting)
            .map_err(|_| io::Error::other("work admission lock poisoned"))
    }

    pub fn acquire(
        &self,
        priority: ReadPriority,
        cancel: &AtomicBool,
        deadline: Option<u64>,
    ) -> io::Result<WorkPermit> {
        check_active(cancel, deadline)?;
        let rank = priority as usize;
        self.0
            .waiting
            .lock()
            .map_err(|_| io::Error::other("work admission lock poisoned"))?[rank] += 1;
        let _waiting = Waiting {
            admission: self.clone(),
            rank,
        };
        loop {
            check_active(cancel, deadline)?;
            let waiting = self
                .0
                .waiting
                .lock()
                .map_err(|_| io::Error::other("work admission lock poisoned"))?;
            if !waiting[..rank].iter().any(|count| *count > 0) {
                match self.0.pool.acquire(
                    Resources {
                        cpu: 1,
                        ..Default::default()
                    },
                    None,
                ) {
                    Ok(permit) => {
                        return Ok(WorkPermit {
                            permit: Some(permit),
                            admission: self.clone(),
                        })
                    }
                    Err(ResourceError::Busy) => {}
                    Err(error) => return Err(error.into_io()),
                }
            }
            let _ = self
                .0
                .changed
                .wait_timeout(waiting, Duration::from_millis(10))
                .map_err(|_| io::Error::other("work admission lock poisoned"))?;
        }
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        let mut waiting = self
            .admission
            .0
            .waiting
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        waiting[self.rank] -= 1;
        self.admission.0.changed.notify_all();
    }
}
impl Drop for WorkPermit {
    fn drop(&mut self) {
        drop(self.permit.take());
        self.admission.0.changed.notify_all();
    }
}

pub fn global_admission() -> &'static WorkAdmission {
    static ADMISSION: LazyLock<WorkAdmission> =
        LazyLock::new(|| WorkAdmission::new(crate::resource_budget::global_pool().clone()));
    &ADMISSION
}
