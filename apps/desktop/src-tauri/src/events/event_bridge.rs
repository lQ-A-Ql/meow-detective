use tauri::{AppHandle, Emitter};
use transport::events::EventEnvelope;

pub fn emit_event<T: serde::Serialize>(
    app: &AppHandle,
    topic: &str,
    event: &EventEnvelope<T>,
) -> tauri::Result<()> {
    app.emit(topic, event)
}
