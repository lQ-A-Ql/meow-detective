use app_services::active_case::ActiveCase;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct AppState {
    pub active_case: Arc<Mutex<Option<ActiveCase>>>,
    pub cancel_tokens: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}
