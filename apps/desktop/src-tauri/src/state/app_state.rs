use app_services::active_case::ActiveCase;
use mcp_client::{McpClient, McpConfig, McpServerConfig};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::info;

use super::task_manager::TaskManager;

/// Type alias for the SQLite connection pool.
pub type DbPool = Pool<SqliteConnectionManager>;

/// Application state shared across Tauri commands.
#[derive(Clone)]
pub struct AppState {
    /// Currently active case (if any).
    pub active_case: Arc<Mutex<Option<ActiveCase>>>,
    /// Manager for background tasks.
    pub task_manager: Arc<TaskManager>,
    /// Database connection pool (initialized when a case is opened).
    pub db_pool: Arc<Mutex<Option<DbPool>>>,
    /// MCP clients (server_id -> client)
    pub mcp_clients: Arc<Mutex<HashMap<String, McpClient>>>,
    /// MCP configuration
    pub mcp_config: Arc<Mutex<McpConfig>>,
    /// MCP config file path
    pub mcp_config_path: PathBuf,
}

impl Default for AppState {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("forensics");
        let mcp_config_path = config_dir.join("mcp-config.json");

        Self {
            active_case: Arc::new(Mutex::new(None)),
            task_manager: Arc::new(TaskManager::new()),
            db_pool: Arc::new(Mutex::new(None)),
            mcp_clients: Arc::new(Mutex::new(HashMap::new())),
            mcp_config: Arc::new(Mutex::new(McpConfig::default())),
            mcp_config_path,
        }
    }
}

impl AppState {
    /// Initialize the database connection pool for the given database path.
    pub fn init_db_pool(&self, db_path: &PathBuf) -> Result<(), String> {
        let manager = SqliteConnectionManager::file(db_path.as_path());
        let pool = Pool::builder()
            .max_size(10)
            .min_idle(Some(2))
            .build(manager)
            .map_err(|e| format!("Failed to create connection pool: {}", e))?;

        {
            let conn = pool.get().map_err(|e| format!("Failed to get connection: {}", e))?;
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA foreign_keys=ON;
                 PRAGMA busy_timeout=5000;
                 PRAGMA synchronous=NORMAL;",
            ).map_err(|e| format!("Failed to set pragmas: {}", e))?;
        }

        let mut guard = self.db_pool.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        *guard = Some(pool);
        Ok(())
    }

    /// Get a connection from the pool.
    pub fn get_connection(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, String> {
        let guard = self.db_pool.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        let pool = guard.as_ref()
            .ok_or("No database pool initialized — is a case open?")?;
        pool.get().map_err(|e| format!("Failed to get connection from pool: {}", e))
    }

    /// Clear the connection pool.
    pub fn clear_db_pool(&self) -> Result<(), String> {
        let mut guard = self.db_pool.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        *guard = None;
        Ok(())
    }

    /// Load MCP configuration from file.
    pub fn load_mcp_config(&self) -> Result<(), String> {
        if !self.mcp_config_path.exists() {
            info!("MCP config file not found, using defaults");
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.mcp_config_path)
            .map_err(|e| format!("Failed to read MCP config: {}", e))?;

        let config: McpConfig = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse MCP config: {}", e))?;

        let mut guard = self.mcp_config.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        *guard = config;

        info!("MCP config loaded from {:?}", self.mcp_config_path);
        Ok(())
    }

    /// Save MCP configuration to file.
    pub fn save_mcp_config(&self) -> Result<(), String> {
        let guard = self.mcp_config.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;

        // Create config directory if it doesn't exist
        if let Some(parent) = self.mcp_config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let content = serde_json::to_string_pretty(&*guard)
            .map_err(|e| format!("Failed to serialize MCP config: {}", e))?;

        std::fs::write(&self.mcp_config_path, content)
            .map_err(|e| format!("Failed to write MCP config: {}", e))?;

        info!("MCP config saved to {:?}", self.mcp_config_path);
        Ok(())
    }

    /// Add an MCP server.
    pub fn add_mcp_server(&self, config: McpServerConfig) -> Result<(), String> {
        let mut guard = self.mcp_config.lock()
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
            let mut guard = self.mcp_config.lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            guard.servers.retain(|s| s.id != server_id);
        }

        // Also disconnect and remove the client
        {
            let mut clients = self.mcp_clients.lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            clients.remove(server_id);
        }

        self.save_mcp_config()
    }

    /// Get MCP server status.
    pub fn get_mcp_server_status(&self, server_id: &str) -> Option<mcp_client::McpServerStatus> {
        let clients = self.mcp_clients.lock().ok()?;
        let client = clients.get(server_id)?;

        Some(mcp_client::McpServerStatus {
            id: server_id.to_string(),
            name: client.config().name.clone(),
            connected: client.is_connected(),
            capabilities: mcp_client::McpCapabilities::default(),
            last_error: None,
        })
    }

    /// Connect to an MCP server.
    pub async fn connect_mcp_server(&self, server_id: &str) -> Result<(), String> {
        let config = {
            let guard = self.mcp_config.lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            guard.servers.iter().find(|s| s.id == server_id).cloned()
                .ok_or_else(|| format!("Server {} not found", server_id))?
        };

        let mut client = McpClient::new(config);
        client.connect().await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        let mut clients = self.mcp_clients.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        clients.insert(server_id.to_string(), client);

        Ok(())
    }

    /// Disconnect from an MCP server.
    pub async fn disconnect_mcp_server(&self, server_id: &str) -> Result<(), String> {
        let mut clients = self.mcp_clients.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;

        if let Some(client) = clients.get_mut(server_id) {
            client.disconnect().await
                .map_err(|e| format!("Failed to disconnect: {}", e))?;
        }

        Ok(())
    }
}
