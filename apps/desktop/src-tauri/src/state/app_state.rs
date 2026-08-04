use app_services::active_case::ActiveCase;
use app_services::bitlocker_runtime::BitLockerUnlockRegistry;
use app_services::bitlocker_service::BitLockerKeyStore;
use mcp_client::{
    validate_mcp_config, validate_mcp_server_config, McpClient, McpConfig, McpServerConfig,
};
use runtime_cache::RuntimeCache;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tracing::info;

use super::task_manager::TaskManager;
use crate::mount_registry::MountRegistry;
use crate::physical_mount_registry::PhysicalMountRegistry;
use app_services::file_service::PreviewRuntimeRegistry;

const APP_CODE_NAME: &str = "Meow_Detective";

pub type SharedMcpClient = Arc<AsyncMutex<McpClient>>;

/// Application state shared across Tauri commands.
#[derive(Clone)]
pub struct AppState {
    /// Currently active case (if any).
    pub active_case: Arc<Mutex<Option<ActiveCase>>>,
    /// Serializes create/open/close/delete transitions across the active case.
    pub case_lifecycle: Arc<AsyncMutex<()>>,
    /// Manager for background tasks.
    pub task_manager: Arc<TaskManager>,
    /// MCP clients (server_id -> client)
    pub mcp_clients: Arc<RwLock<HashMap<String, SharedMcpClient>>>,
    /// MCP configuration
    pub mcp_config: Arc<Mutex<McpConfig>>,
    /// MCP config file path
    pub mcp_config_path: PathBuf,
    /// Application settings file path
    pub app_settings_path: PathBuf,
    /// Runtime cache for ephemeral preview and query handles
    pub runtime_cache: Arc<Mutex<RuntimeCache>>,
    /// Source-scoped evidence runtimes and opaque preview sessions.
    pub preview_runtime: Arc<PreviewRuntimeRegistry>,
    /// Verified BitLocker cipher state. Credentials are never stored here.
    pub bitlocker_runtime: Arc<BitLockerUnlockRegistry>,
    /// OS-protected persistence for verified FVEK/tweak packages.
    pub bitlocker_key_store: Arc<dyn BitLockerKeyStore>,
    /// Active user-mode read-only logical mounts.
    pub mount_registry: Arc<MountRegistry>,
    /// Active loopback iSCSI read-only physical-disk mounts.
    pub physical_mount_registry: Arc<PhysicalMountRegistry>,
}

impl Default for AppState {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_CODE_NAME);
        let mcp_config_path = config_dir.join("mcp-config.json");
        let app_settings_path = config_dir.join("app-settings.json");
        let runtime_cache = RuntimeCache::open_in_memory().expect("runtime cache must initialize");

        Self {
            active_case: Arc::new(Mutex::new(None)),
            case_lifecycle: Arc::new(AsyncMutex::new(())),
            task_manager: Arc::new(TaskManager::new()),
            mcp_clients: Arc::new(RwLock::new(HashMap::new())),
            mcp_config: Arc::new(Mutex::new(McpConfig::default())),
            mcp_config_path,
            app_settings_path,
            runtime_cache: Arc::new(Mutex::new(runtime_cache)),
            preview_runtime: Arc::new(PreviewRuntimeRegistry::default()),
            bitlocker_runtime: Arc::new(BitLockerUnlockRegistry::default()),
            bitlocker_key_store: crate::bitlocker_key_store::platform_bitlocker_key_store(),
            mount_registry: Arc::new(MountRegistry::default()),
            physical_mount_registry: Arc::new(PhysicalMountRegistry::default()),
        }
    }
}

impl AppState {
    pub async fn get_mcp_client(&self, server_id: &str) -> Result<SharedMcpClient, String> {
        let clients = self.mcp_clients.read().await;
        clients
            .get(server_id)
            .cloned()
            .ok_or_else(|| format!("Server {} not connected", server_id))
    }

    pub async fn replace_mcp_client(
        &self,
        server_id: String,
        client: McpClient,
    ) -> Result<(), String> {
        let mut clients = self.mcp_clients.write().await;
        clients.insert(server_id, Arc::new(AsyncMutex::new(client)));
        Ok(())
    }

    pub async fn remove_mcp_client(
        &self,
        server_id: &str,
    ) -> Result<Option<SharedMcpClient>, String> {
        let mut clients = self.mcp_clients.write().await;
        Ok(clients.remove(server_id))
    }

    pub async fn sync_mcp_clients_with_config(&self) -> Result<(), String> {
        let allowed_ids: HashSet<String> = {
            let guard = self
                .mcp_config
                .lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            guard
                .servers
                .iter()
                .map(|server| server.id.clone())
                .collect()
        };

        let stale_clients = {
            let clients = self.mcp_clients.read().await;
            clients
                .keys()
                .filter(|id| !allowed_ids.contains(*id))
                .cloned()
                .collect::<Vec<_>>()
        };

        for server_id in stale_clients {
            if let Some(client) = self.remove_mcp_client(&server_id).await? {
                let mut client = client.lock().await;
                let _ = client.disconnect().await;
            }
        }

        Ok(())
    }

    /// Initialize database pragmas on the active case connection.
    pub fn init_db_pragmas(&self) -> Result<(), String> {
        let conn = self.get_connection()?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=30000;
             PRAGMA synchronous=NORMAL;",
        )
        .map_err(|e| format!("Failed to set pragmas: {}", e))?;
        Ok(())
    }

    /// Get a fresh connection to the active case's database.
    /// Opens a new connection each time — cheap in WAL mode, eliminates all
    /// shared-state and lock-contention issues from the old r2d2 pool.
    pub fn get_connection(&self) -> Result<rusqlite::Connection, String> {
        let guard = self
            .active_case
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        let active = guard
            .as_ref()
            .ok_or("No active case — open or create a case first")?;
        let db_path = active.db_path();
        drop(guard);
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        // Each new connection needs its own busy timeout. WAL mode is persistent,
        // but a generous timeout is required so concurrent readers/writers
        // (e.g. parallel governance/timeline queries that lazily project MACB
        // events) queue instead of failing immediately with "database is locked".
        conn.execute_batch("PRAGMA busy_timeout=30000;")
            .map_err(|e| format!("Failed to set busy_timeout: {}", e))?;
        Ok(conn)
    }

    /// Clear the database state on case close.
    pub fn clear_db_state(&self) -> Result<(), String> {
        let mut guard = self
            .active_case
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        *guard = None;
        Ok(())
    }

    pub fn clear_runtime_cache_for_case(&self, case_id: &str) -> Result<u64, String> {
        let cache = self
            .runtime_cache
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        cache
            .clear_case(case_id)
            .map_err(|e| format!("Failed to clear runtime cache: {}", e))
    }

    pub fn clear_preview_runtime_for_case(&self, case_id: &str) -> Result<(), String> {
        self.preview_runtime
            .invalidate_case(case_id)
            .map_err(|error| format!("Failed to clear preview runtime: {error}"))
    }

    pub fn clear_preview_runtime_for_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), String> {
        self.preview_runtime
            .invalidate_source(case_id, data_source_id)
            .map_err(|error| format!("Failed to clear preview runtime: {error}"))
    }

    pub fn retire_preview_case(&self, case_id: &str, timeout: Duration) -> Result<bool, String> {
        let drained = self
            .preview_runtime
            .retire_case_and_drain(case_id, timeout)
            .map_err(|error| format!("Failed to retire preview runtime: {error}"))?;
        if drained {
            self.bitlocker_runtime
                .invalidate_case(case_id)
                .map_err(|error| format!("Failed to clear BitLocker runtime: {error}"))?;
        }
        Ok(drained)
    }

    pub fn cleanup_mounts_for_case(&self, case_id: &str) -> Result<(), String> {
        let logical = self
            .mount_registry
            .cleanup_case(case_id)
            .map_err(|error| format!("Failed to clean up logical image mounts: {error}"));
        let physical = self
            .physical_mount_registry
            .cleanup_case(case_id)
            .map_err(|error| format!("Failed to clean up physical image mounts: {error}"));
        logical.and(physical)
    }

    pub fn cleanup_mounts_for_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), String> {
        let logical = self
            .mount_registry
            .cleanup_source(case_id, data_source_id)
            .map_err(|error| format!("Failed to clean up logical data-source mounts: {error}"));
        let physical = self
            .physical_mount_registry
            .cleanup_source(case_id, data_source_id)
            .map_err(|error| format!("Failed to clean up physical data-source mounts: {error}"));
        logical.and(physical)
    }

    pub fn retire_preview_source(
        &self,
        case_id: &str,
        data_source_id: &str,
        timeout: Duration,
    ) -> Result<bool, String> {
        let drained = self
            .preview_runtime
            .retire_source_and_drain(case_id, data_source_id, timeout)
            .map_err(|error| format!("Failed to retire preview runtime: {error}"))?;
        if drained {
            self.bitlocker_runtime
                .invalidate_source(case_id, data_source_id)
                .map_err(|error| format!("Failed to clear BitLocker runtime: {error}"))?;
        }
        Ok(drained)
    }

    pub fn reactivate_preview_case(&self, case_id: &str) -> Result<(), String> {
        self.preview_runtime
            .reactivate_case(case_id)
            .map_err(|error| format!("Failed to reactivate preview runtime: {error}"))
    }

    pub fn reactivate_preview_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), String> {
        self.preview_runtime
            .reactivate_source(case_id, data_source_id)
            .map_err(|error| format!("Failed to reactivate preview runtime: {error}"))
    }

    /// Load MCP configuration from file.
    pub fn load_mcp_config(&self) -> Result<(), String> {
        if !self.mcp_config_path.exists() {
            info!("MCP config file not found, using defaults");
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.mcp_config_path)
            .map_err(|e| format!("Failed to read MCP config: {}", e))?;

        let mut config: McpConfig = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse MCP config: {}", e))?;
        validate_mcp_config(&mut config).map_err(|e| format!("Invalid MCP config: {}", e))?;

        let mut guard = self
            .mcp_config
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        *guard = config;

        info!("MCP config loaded from {:?}", self.mcp_config_path);
        Ok(())
    }

    /// Save MCP configuration to file.
    pub fn save_mcp_config(&self) -> Result<(), String> {
        let mut config = {
            let guard = self
                .mcp_config
                .lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            guard.clone()
        };
        validate_mcp_config(&mut config).map_err(|e| format!("Invalid MCP config: {}", e))?;

        // Create config directory if it doesn't exist
        if let Some(parent) = self.mcp_config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let content = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize MCP config: {}", e))?;

        std::fs::write(&self.mcp_config_path, content)
            .map_err(|e| format!("Failed to write MCP config: {}", e))?;

        {
            let mut guard = self
                .mcp_config
                .lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            *guard = config;
        }

        info!("MCP config saved to {:?}", self.mcp_config_path);
        Ok(())
    }

    /// Add an MCP server.
    pub fn add_mcp_server(&self, mut config: McpServerConfig) -> Result<(), String> {
        validate_mcp_server_config(&mut config)
            .map_err(|e| format!("Invalid MCP server: {}", e))?;

        let mut guard = self
            .mcp_config
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;

        // Check for duplicate ID
        if guard.servers.iter().any(|s| s.id == config.id) {
            return Err(format!("Server with ID {} already exists", config.id));
        }

        guard.servers.push(config);
        drop(guard);

        self.save_mcp_config()
    }

    /// Remove an MCP server.
    pub fn remove_mcp_server(&self, server_id: &str) -> Result<(), String> {
        {
            let mut guard = self
                .mcp_config
                .lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            guard.servers.retain(|s| s.id != server_id);
        }

        self.save_mcp_config()
    }

    /// Get MCP server status.
    pub fn get_mcp_server_status(&self, server_id: &str) -> Option<mcp_client::McpServerStatus> {
        let handle = tokio::runtime::Handle::try_current().ok()?;
        tokio::task::block_in_place(|| {
            handle.block_on(async {
                let client = self.get_mcp_client(server_id).await.ok()?;
                let client = client.lock().await;

                Some(mcp_client::McpServerStatus {
                    id: server_id.to_string(),
                    name: client.config().name.clone(),
                    connected: client.is_connected(),
                    capabilities: client.capabilities().cloned().unwrap_or_default(),
                    last_error: None,
                })
            })
        })
    }

    /// Connect to an MCP server.
    pub async fn connect_mcp_server(&self, server_id: &str) -> Result<(), String> {
        let config = {
            let guard = self
                .mcp_config
                .lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            guard
                .servers
                .iter()
                .find(|s| s.id == server_id)
                .cloned()
                .ok_or_else(|| format!("Server {} not found", server_id))?
        };

        let mut client = McpClient::new(config);
        client
            .connect()
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;
        self.replace_mcp_client(server_id.to_string(), client)
            .await?;

        Ok(())
    }

    /// Disconnect from an MCP server.
    pub async fn disconnect_mcp_server(&self, server_id: &str) -> Result<(), String> {
        if let Some(client) = self.remove_mcp_client(server_id).await? {
            let mut client = client.lock().await;
            client
                .disconnect()
                .await
                .map_err(|e| format!("Failed to disconnect: {}", e))?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/state/app_state.rs"]
mod tests;
