use super::{handler::router, McpHostError};
use crate::state::AppState;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use transport::dto::mcp_host::{McpHostSettingsDto, McpHostStatusDto};

const ADDRESS: &str = "127.0.0.1:3001";

struct RunningServer {
    address: SocketAddr,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

pub struct McpHostRuntime {
    settings: Mutex<McpHostSettingsDto>,
    running: Mutex<Option<RunningServer>>,
    last_error: Mutex<Option<String>>,
    operation: tokio::sync::Mutex<()>,
    live: Arc<AtomicBool>,
    pub(super) requests: Arc<Semaphore>,
}

impl Default for McpHostRuntime {
    fn default() -> Self {
        Self {
            settings: Mutex::new(McpHostSettingsDto::default()),
            running: Mutex::new(None),
            last_error: Mutex::new(None),
            operation: tokio::sync::Mutex::new(()),
            live: Arc::new(AtomicBool::new(false)),
            requests: Arc::new(Semaphore::new(2)),
        }
    }
}

impl McpHostRuntime {
    pub fn status(&self) -> Result<McpHostStatusDto, McpHostError> {
        let running = self.running.lock().map_err(|_| McpHostError::State)?;
        let endpoint = running
            .as_ref()
            .map(|server| server.address.to_string())
            .unwrap_or_else(|| ADDRESS.into());
        Ok(McpHostStatusDto {
            settings: self
                .settings
                .lock()
                .map_err(|_| McpHostError::State)?
                .clone(),
            running: self.live.load(Ordering::Acquire),
            endpoint: format!("http://{endpoint}/mcp"),
            last_error: self
                .last_error
                .lock()
                .map_err(|_| McpHostError::State)?
                .clone(),
        })
    }

    pub fn tool_enabled(&self, name: &str) -> Result<bool, McpHostError> {
        let settings = self.settings.lock().map_err(|_| McpHostError::State)?;
        Ok(settings.enabled
            && !settings
                .disabled_tools
                .iter()
                .any(|disabled| disabled == name))
    }

    pub async fn initialize(&self, state: AppState) {
        let _operation = self.operation.lock().await;
        let settings = load_settings(&state);
        match settings {
            Ok(mut settings) => {
                if let Err(error) = app_services::mcp_host_service::validate_settings(&mut settings)
                {
                    self.fail_closed();
                    self.record_error(error.to_string());
                    return;
                }
                if let Ok(mut guard) = self.settings.lock() {
                    *guard = settings.clone();
                }
                if settings.enabled {
                    if let Err(error) = self
                        .start_on(state, SocketAddr::from(([127, 0, 0, 1], 3001)))
                        .await
                    {
                        self.record_error(error.to_string());
                    }
                }
            }
            Err(error) => {
                self.fail_closed();
                self.record_error(error.to_string());
            }
        }
    }

    pub async fn configure(
        &self,
        state: AppState,
        mut settings: McpHostSettingsDto,
    ) -> Result<McpHostStatusDto, McpHostError> {
        let _operation = self.operation.lock().await;
        app_services::mcp_host_service::validate_settings(&mut settings)?;
        let was_running = self.live.load(Ordering::Acquire);
        if settings.enabled && !was_running {
            if let Err(error) = self
                .start_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 3001)))
                .await
            {
                self.record_error(error.to_string());
                return Err(error);
            }
        }
        if let Err(error) = persist_settings(&state, &settings) {
            if !was_running {
                self.stop().await;
            }
            self.record_error(error.to_string());
            return Err(error);
        }
        *self.settings.lock().map_err(|_| McpHostError::State)? = settings.clone();
        if !settings.enabled {
            self.stop().await;
        }
        self.record_error_clear();
        self.status()
    }

    pub(super) async fn start_on(
        &self,
        state: AppState,
        address: SocketAddr,
    ) -> Result<(), McpHostError> {
        if self.live.load(Ordering::Acquire) {
            return Ok(());
        }
        let listener = TcpListener::bind(address)
            .await
            .map_err(McpHostError::Bind)?;
        let address = listener.local_addr()?;
        let cancel = CancellationToken::new();
        let app = router(state, cancel.clone());
        let shutdown = cancel.clone();
        let live = self.live.clone();
        live.store(true, Ordering::Release);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await;
            live.store(false, Ordering::Release);
        });
        *self.running.lock().map_err(|_| McpHostError::State)? = Some(RunningServer {
            address,
            cancel,
            task,
        });
        Ok(())
    }

    pub fn shutdown(&self) {
        self.live.store(false, Ordering::Release);
        self.abort_task();
    }

    async fn stop(&self) {
        self.live.store(false, Ordering::Release);
        if let Some(task) = self.abort_task() {
            let _ = task.await;
        }
    }

    fn abort_task(&self) -> Option<JoinHandle<()>> {
        if let Ok(mut guard) = self.running.lock() {
            if let Some(server) = guard.take() {
                server.cancel.cancel();
                server.task.abort();
                return Some(server.task);
            }
        }
        None
    }

    fn record_error(&self, error: String) {
        if let Ok(mut guard) = self.last_error.lock() {
            *guard = Some(error);
        }
    }

    fn fail_closed(&self) {
        self.shutdown();
        if let Ok(mut settings) = self.settings.lock() {
            settings.enabled = false;
        }
    }

    fn record_error_clear(&self) {
        if let Ok(mut guard) = self.last_error.lock() {
            *guard = None;
        }
    }
}

fn settings_path(state: &AppState) -> std::path::PathBuf {
    state.mcp_config_path.with_file_name("mcp-host.json")
}

fn load_settings(state: &AppState) -> Result<McpHostSettingsDto, McpHostError> {
    use std::io::Read;
    let file = match std::fs::File::open(settings_path(state)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(McpHostSettingsDto::default())
        }
        Err(error) => return Err(McpHostError::Io(error)),
    };
    let mut bytes = Vec::new();
    file.take(32 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 {
        return Err(McpHostError::SettingsTooLarge);
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn persist_settings(state: &AppState, settings: &McpHostSettingsDto) -> Result<(), McpHostError> {
    use std::io::Write;
    let path = settings_path(state);
    let parent = path.parent().ok_or(McpHostError::State)?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&serde_json::to_vec_pretty(settings)?)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| McpHostError::Io(error.error))?;
    Ok(())
}
