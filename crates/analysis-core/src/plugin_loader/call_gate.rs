use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

type Gates = HashMap<usize, Weak<Mutex<()>>>;

pub(super) fn for_entry(entry: usize) -> Result<Arc<Mutex<()>>, String> {
    static GATES: OnceLock<Mutex<Gates>> = OnceLock::new();
    let mut gates = GATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "plugin call gate registry is poisoned".to_string())?;
    gates.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = gates.get(&entry).and_then(Weak::upgrade) {
        return Ok(gate);
    }
    let gate = Arc::new(Mutex::new(()));
    gates.insert(entry, Arc::downgrade(&gate));
    Ok(gate)
}

#[cfg(test)]
#[path = "../../tests/unit/plugin_call_gate.rs"]
mod tests;
