//! Optional plugin action channel (`meow_plugin_action`, ABI doc §3
//! optional export): symbol-existence probing, the `describe` action, and
//! action invocation over the same per-plugin serialization and buffer
//! discipline as extraction.

use super::extractor::PluginExtractor;
use plugin_api::MeowStatus;
use serde_json::Value;

impl PluginExtractor {
    /// Whether the plugin exports the optional action channel
    /// (`meow_plugin_action`, ABI doc §3 optional export).
    pub fn has_actions(&self) -> bool {
        self.shared.library.action_fn().is_some()
    }

    /// The plugin's self-described action list (the `describe` action).
    /// A plugin without the action channel yields an empty list (graceful
    /// degradation, never an error).
    pub fn describe_actions(&self) -> Result<Value, String> {
        if !self.has_actions() {
            return Ok(Value::Array(Vec::new()));
        }
        let response = self.call_action("describe", &Value::Object(serde_json::Map::new()))?;
        Ok(response
            .get("actions")
            .cloned()
            .unwrap_or(Value::Array(Vec::new())))
    }

    /// Invoke one plugin action. `params` is embedded into the
    /// self-describing request envelope `{"action": ..., "params": ...}`;
    /// the action list is discovered through [`Self::describe_actions`].
    ///
    /// The response payload is returned as parsed JSON. Action payloads may
    /// carry secrets (e.g. recovered keys) by explicit action design; the
    /// caller owns their discipline — they are never logged here.
    pub fn call_action(&self, action: &str, params: &Value) -> Result<Value, String> {
        let request = serde_json::json!({ "action": action, "params": params });
        self.call_action_value(&request)
    }

    fn call_action_value(&self, request: &Value) -> Result<Value, String> {
        let Some(action_fn) = self.shared.library.action_fn() else {
            return Err(format!(
                "plugin {} does not export the action channel",
                self.shared.id
            ));
        };
        // Same serialization discipline as extraction (contract §3).
        let _serial = self
            .call_lock
            .lock()
            .map_err(|_| format!("plugin {} call lock poisoned", self.shared.id))?;
        let body = serde_json::to_vec(request).map_err(|error| {
            format!(
                "plugin {} action request is not serializable: {error}",
                self.shared.id
            )
        })?;
        // SAFETY: the request buffer is host-owned and outlives the call;
        // the contract forbids the plugin from retaining it. catch_unwind is
        // defense in depth — the plugin must self-catch (contract §8).
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            action_fn(body.as_ptr(), body.len() as u64)
        }));
        let response =
            outcome.map_err(|_| format!("plugin {} panicked during action", self.shared.id))?;
        self.action_response_value(response)
    }

    fn action_response_value(
        &self,
        response: plugin_api::MeowExtractResponse,
    ) -> Result<Value, String> {
        let buffers = self.take_buffers(&response);
        if response.status != MeowStatus::Ok {
            let detail = buffers
                .error
                .unwrap_or_else(|| "no error message".to_string());
            return Err(format!(
                "plugin {} action returned {:?}: {}",
                self.shared.id, response.status, detail
            ));
        }
        let payload = buffers.payload.ok_or_else(|| {
            format!(
                "plugin {} action returned Ok without a payload",
                self.shared.id
            )
        })?;
        let text = String::from_utf8(payload)
            .map_err(|_| format!("plugin {} action payload is not UTF-8", self.shared.id))?;
        serde_json::from_str(&text).map_err(|error| {
            format!(
                "plugin {} action payload is not valid JSON: {error}",
                self.shared.id
            )
        })
    }
}
