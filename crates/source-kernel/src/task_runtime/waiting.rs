use super::registry::TaskRegistry;
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub(super) fn wait_for_group_prefix(
    registry: &TaskRegistry,
    case_id: &str,
    prefix: &str,
    cancel: &AtomicBool,
) -> io::Result<()> {
    let mut state = registry
        .state
        .lock()
        .map_err(|error| io::Error::other(error.to_string()))?;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "task wait cancelled",
            ));
        }
        if !state
            .tasks
            .values()
            .filter_map(|entry| entry.scope.as_ref())
            .any(|scope| scope.case_id == case_id && scope.group_id.starts_with(prefix))
        {
            return Ok(());
        }
        (state, _) = registry
            .changed
            .wait_timeout(state, Duration::from_millis(100))
            .map_err(|error| io::Error::other(error.to_string()))?;
    }
}
