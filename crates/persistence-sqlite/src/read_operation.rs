use crate::{DbError, DbResult};
use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadStopReason {
    Cancelled,
    TimedOut,
}

#[derive(Default)]
pub struct ReadCancellation {
    reason: AtomicU8,
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<ReadCancellation>>> = const { RefCell::new(None) };
}

/// A synchronous read scope belongs to its creating thread and restores nested scopes.
pub struct ReadScope {
    previous: Option<Arc<ReadCancellation>>,
    _thread: PhantomData<Rc<()>>,
}

impl ReadCancellation {
    pub fn cancel(&self) {
        let _ = self
            .reason
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
    }

    pub fn time_out(&self) {
        let _ = self
            .reason
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
    }

    pub fn reason(&self) -> Option<ReadStopReason> {
        match self.reason.load(Ordering::Acquire) {
            0 => None,
            1 => Some(ReadStopReason::Cancelled),
            2 => Some(ReadStopReason::TimedOut),
            _ => unreachable!("invalid read cancellation state"),
        }
    }

    pub fn enter(self: &Arc<Self>) -> ReadScope {
        ReadScope {
            previous: CURRENT.with(|current| current.replace(Some(self.clone()))),
            _thread: PhantomData,
        }
    }
}

impl Drop for ReadScope {
    fn drop(&mut self) {
        CURRENT.with(|current| *current.borrow_mut() = self.previous.take());
    }
}

pub fn ensure_active() -> DbResult<()> {
    match CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|control| control.reason())
    }) {
        Some(ReadStopReason::Cancelled) => Err(DbError::ReadCancelled),
        Some(ReadStopReason::TimedOut) => Err(DbError::ReadTimedOut),
        None => Ok(()),
    }
}

pub(crate) fn configure_connection(connection: &rusqlite::Connection) -> DbResult<()> {
    ensure_active()?;
    if CURRENT.with(|current| current.borrow().is_none()) {
        return Ok(());
    }
    // Callbacks consult the executing thread; a reused connection never captures an old request.
    connection.progress_handler(1000, Some(|| ensure_active().is_err()));
    connection.busy_handler(Some(read_busy))?;
    Ok(())
}

fn read_busy(attempt: i32) -> bool {
    if ensure_active().is_err() || attempt >= 6000 {
        return false;
    }
    std::thread::sleep(Duration::from_millis(5));
    ensure_active().is_ok()
}
