use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

pub type TaskResult = Result<(), String>;

const MAX_COMPLETED_RESULTS: usize = 1_024;

pub(super) struct TaskEntry {
    pub(super) cancel_token: Arc<AtomicBool>,
    pub(super) started_at: Instant,
    pub(super) scope: Option<TaskScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskScope {
    pub(super) case_id: String,
    pub(super) data_source_id: Option<String>,
    pub(super) group_id: String,
}

impl TaskScope {
    pub fn case(case_id: impl Into<String>, group_id: impl Into<String>) -> Self {
        Self {
            case_id: case_id.into(),
            data_source_id: None,
            group_id: group_id.into(),
        }
    }

    pub fn data_source(
        case_id: impl Into<String>,
        data_source_id: impl Into<String>,
        group_id: impl Into<String>,
    ) -> Self {
        Self {
            case_id: case_id.into(),
            data_source_id: Some(data_source_id.into()),
            group_id: group_id.into(),
        }
    }
}

#[derive(Debug)]
pub enum TaskRegistrationError {
    DuplicateTaskId(String),
    RetiredCase(String),
    RetiredDataSource {
        case_id: String,
        data_source_id: String,
    },
    Spawn(std::io::Error),
    HeavyQueueClosed(String),
}

impl fmt::Display for TaskRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTaskId(task_id) => {
                write!(formatter, "background task '{task_id}' is already active")
            }
            Self::RetiredCase(case_id) => {
                write!(
                    formatter,
                    "case '{case_id}' is no longer accepting background tasks"
                )
            }
            Self::RetiredDataSource {
                case_id,
                data_source_id,
            } => write!(
                formatter,
                "data source '{data_source_id}' in case '{case_id}' is no longer accepting background tasks"
            ),
            Self::Spawn(error) => write!(formatter, "failed to spawn background task: {error}"),
            Self::HeavyQueueClosed(task_id) => {
                write!(
                    formatter,
                    "heavy background task '{task_id}' could not be queued"
                )
            }
        }
    }
}

impl std::error::Error for TaskRegistrationError {}

#[derive(Default)]
pub(super) struct RegistryState {
    pub(super) tasks: HashMap<String, TaskEntry>,
    pub(super) completed: HashMap<String, TaskResult>,
    pub(super) completed_order: VecDeque<String>,
    pub(super) retired_cases: HashSet<String>,
    pub(super) retired_sources: HashSet<(String, String)>,
}

#[derive(Default)]
pub(super) struct TaskRegistry {
    pub(super) state: Mutex<RegistryState>,
    pub(super) changed: Condvar,
}

impl TaskRegistry {
    pub(super) fn complete(&self, task_id: String, result: TaskResult) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.tasks.remove(&task_id);
        if state.completed.insert(task_id.clone(), result).is_none() {
            state.completed_order.push_back(task_id);
        }
        while state.completed.len() > MAX_COMPLETED_RESULTS {
            let Some(oldest) = state.completed_order.pop_front() else {
                break;
            };
            state.completed.remove(&oldest);
            tracing::warn!(
                task_id = oldest,
                "Evicted an unobserved background task result from the bounded registry"
            );
        }
        self.changed.notify_all();
    }
}

pub(super) fn validate_registration(
    state: &RegistryState,
    task_id: &str,
    scope: Option<&TaskScope>,
) -> Result<(), TaskRegistrationError> {
    if state.tasks.contains_key(task_id) {
        return Err(TaskRegistrationError::DuplicateTaskId(task_id.to_string()));
    }
    let Some(scope) = scope else {
        return Ok(());
    };
    if state.retired_cases.contains(&scope.case_id) {
        return Err(TaskRegistrationError::RetiredCase(scope.case_id.clone()));
    }
    if let Some(data_source_id) = &scope.data_source_id {
        if state
            .retired_sources
            .contains(&(scope.case_id.clone(), data_source_id.clone()))
        {
            return Err(TaskRegistrationError::RetiredDataSource {
                case_id: scope.case_id.clone(),
                data_source_id: data_source_id.clone(),
            });
        }
    }
    Ok(())
}

pub(super) fn cancel_and_collect(
    tasks: &HashMap<String, TaskEntry>,
    predicate: impl Fn(&TaskScope) -> bool,
) -> Vec<String> {
    tasks
        .iter()
        .filter_map(|(task_id, entry)| {
            let matches = entry.scope.as_ref().is_some_and(&predicate);
            if matches {
                entry.cancel_token.store(true, Ordering::Release);
            }
            matches.then_some(task_id.clone())
        })
        .collect()
}

pub(super) fn thread_name(task_id: &str) -> String {
    let bounded = task_id.chars().take(40).collect::<String>();
    format!("meow-task-{bounded}")
}
