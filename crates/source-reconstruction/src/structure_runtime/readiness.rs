use std::collections::{BTreeMap, BTreeSet};

use domain::structure::ActivityState;

use super::{ActivityDag, RuntimeError};

#[derive(Default)]
pub struct Readiness {
    outstanding: BTreeMap<String, usize>,
    dependents: BTreeMap<String, BTreeSet<String>>,
    ready: BTreeSet<String>,
    blocked: BTreeSet<String>,
}

impl Readiness {
    pub fn register(
        &mut self,
        keys: &BTreeSet<String>,
        dag: &ActivityDag,
        states: &BTreeMap<String, ActivityState>,
    ) -> Result<(), RuntimeError> {
        for key in keys {
            let definition = dag.get(key).ok_or(RuntimeError::MissingDependency)?;
            let mut outstanding = 0usize;
            let mut failed = false;
            for dependency in &definition.dependencies {
                let state = states
                    .get(dependency)
                    .ok_or(RuntimeError::MissingDependency)?;
                match state {
                    ActivityState::Succeeded => {}
                    ActivityState::Failed | ActivityState::Blocked | ActivityState::Cancelled => {
                        failed = true;
                    }
                    _ => {
                        outstanding += 1;
                        self.dependents
                            .entry(dependency.clone())
                            .or_default()
                            .insert(key.clone());
                    }
                }
            }
            if failed {
                self.blocked.insert(key.clone());
            } else if outstanding == 0 {
                self.ready.insert(key.clone());
            }
            self.outstanding.insert(key.clone(), outstanding);
        }
        Ok(())
    }

    pub fn completed(&mut self, key: &str, state: ActivityState) {
        assert!(
            state.is_terminal(),
            "dependency completion requires a terminal state"
        );
        self.ready.remove(key);
        self.blocked.remove(key);
        self.outstanding.remove(key);
        for dependent in self.dependents.remove(key).unwrap_or_default() {
            if !self.outstanding.contains_key(&dependent) {
                continue;
            }
            if state == ActivityState::Succeeded {
                let count = self
                    .outstanding
                    .get_mut(&dependent)
                    .expect("dependent activity is absent");
                *count = count
                    .checked_sub(1)
                    .expect("activity dependency count underflow");
                if *count == 0 && !self.blocked.contains(&dependent) {
                    self.ready.insert(dependent);
                }
            } else {
                self.ready.remove(&dependent);
                self.blocked.insert(dependent);
            }
        }
    }

    pub fn pop_ready(&mut self) -> Option<String> {
        self.ready.pop_first()
    }

    pub fn pop_blocked(&mut self) -> Option<String> {
        self.blocked.pop_first()
    }
}
