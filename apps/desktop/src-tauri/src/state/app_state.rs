use app_services::active_case::ActiveCase;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct AppState {
    pub active_case: Arc<Mutex<Option<ActiveCase>>>,
}
