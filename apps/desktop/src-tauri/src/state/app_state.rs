use app_services::active_case::ActiveCase;
use app_services::bitlocker_runtime::BitLockerUnlockRegistry;
use app_services::bitlocker_service::BitLockerKeyStore;
use runtime_cache::RuntimeCache;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::RwLock;

use super::bitlocker_dictionary::BitLockerDictionaryAttackRegistry;
use super::task_manager::TaskManager;
use crate::emulation_registry::EmulationRegistry;
use crate::mount_registry::MountRegistry;
use crate::physical_mount_registry::PhysicalMountRegistry;
use app_services::file_service::PreviewRuntimeRegistry;

const APP_CODE_NAME: &str = "Meow_Detective";

/// Application state shared across Tauri commands.
#[derive(Clone)]
pub struct AppState {
    pub(crate) app_update: Arc<crate::app_update::UpdateRuntime>,
    /// Currently active case (if any).
    pub active_case: Arc<Mutex<Option<ActiveCase>>>,
    /// Bounded read-only pool for case metadata queries and previews.
    pub read_pool: super::case_database::SharedCaseReadPool,
    /// Serializes create/open/close/delete transitions across the active case.
    pub case_lifecycle: Arc<RwLock<()>>,
    /// Manager for background tasks.
    pub task_manager: Arc<TaskManager>,
    /// Embedded MCP host settings file path.
    pub mcp_host_settings_path: PathBuf,
    /// Embedded loopback MCP listener and persisted tool policy.
    pub mcp_host: Arc<crate::mcp_host::McpHostRuntime>,
    /// Application settings file path
    pub app_settings_path: PathBuf,
    /// Runtime cache for ephemeral preview and query handles
    pub runtime_cache: Arc<Mutex<RuntimeCache>>,
    /// Source-scoped evidence runtimes and opaque preview sessions.
    pub preview_runtime: Arc<PreviewRuntimeRegistry>,
    pub registry_browser: Arc<app_services::registry_browser_service::RegistryBrowserRegistry>,
    /// Verified BitLocker cipher state. Credentials are never stored here.
    pub bitlocker_runtime: Arc<BitLockerUnlockRegistry>,
    /// OS-protected persistence for verified FVEK/tweak packages.
    pub bitlocker_key_store: Arc<dyn BitLockerKeyStore>,
    /// In-memory progress and terminal state for BitLocker dictionary jobs.
    pub bitlocker_dictionary_attacks: Arc<BitLockerDictionaryAttackRegistry>,
    /// Active user-mode read-only logical mounts.
    pub mount_registry: Arc<MountRegistry>,
    /// Active loopback iSCSI read-only physical-disk mounts.
    pub physical_mount_registry: Arc<PhysicalMountRegistry>,
    /// Active application-owned copy-on-write emulation sessions.
    pub emulation_registry: Arc<EmulationRegistry>,
}

impl Default for AppState {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_CODE_NAME);
        let mcp_host_settings_path = config_dir.join("mcp-host.json");
        let app_settings_path = config_dir.join("app-settings.json");
        let runtime_cache = RuntimeCache::open_in_memory().expect("runtime cache must initialize");

        Self {
            app_update: Arc::new(crate::app_update::UpdateRuntime::default()),
            active_case: Arc::new(Mutex::new(None)),
            read_pool: Arc::new(Mutex::new(None)),
            case_lifecycle: Arc::new(RwLock::new(())),
            task_manager: Arc::new(TaskManager::new()),
            mcp_host_settings_path,
            mcp_host: Arc::new(crate::mcp_host::McpHostRuntime::default()),
            app_settings_path,
            runtime_cache: Arc::new(Mutex::new(runtime_cache)),
            preview_runtime: Arc::new(PreviewRuntimeRegistry::default()),
            registry_browser: Arc::new(
                app_services::registry_browser_service::RegistryBrowserRegistry::default(),
            ),
            bitlocker_runtime: Arc::new(BitLockerUnlockRegistry::default()),
            bitlocker_key_store: crate::bitlocker_key_store::platform_bitlocker_key_store(),
            bitlocker_dictionary_attacks: Arc::new(BitLockerDictionaryAttackRegistry::default()),
            mount_registry: Arc::new(MountRegistry::default()),
            physical_mount_registry: Arc::new(PhysicalMountRegistry::default()),
            emulation_registry: Arc::new(EmulationRegistry::default()),
        }
    }
}

impl AppState {
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
        self.registry_browser
            .invalidate_case(case_id)
            .map_err(|error| error.to_string())?;
        self.preview_runtime
            .invalidate_case(case_id)
            .map_err(|error| format!("Failed to clear preview runtime: {error}"))
    }

    pub fn clear_preview_runtime_for_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), String> {
        self.registry_browser
            .invalidate_source(case_id, data_source_id)
            .map_err(|error| error.to_string())?;
        self.preview_runtime
            .invalidate_source(case_id, data_source_id)
            .map_err(|error| format!("Failed to clear preview runtime: {error}"))
    }

    pub fn retire_preview_case(&self, case_id: &str, timeout: Duration) -> Result<bool, String> {
        self.registry_browser
            .invalidate_case(case_id)
            .map_err(|error| error.to_string())?;
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
        let emulation = self
            .emulation_registry
            .cleanup_case_on_large_stack(case_id)
            .map_err(|error| format!("Failed to clean up emulation sessions: {error}"));
        logical.and(physical).and(emulation)
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
        let emulation = self
            .emulation_registry
            .cleanup_source_on_large_stack(case_id, data_source_id)
            .map_err(|error| format!("Failed to clean up data-source emulation sessions: {error}"));
        logical.and(physical).and(emulation)
    }

    pub fn retire_preview_source(
        &self,
        case_id: &str,
        data_source_id: &str,
        timeout: Duration,
    ) -> Result<bool, String> {
        self.registry_browser
            .invalidate_source(case_id, data_source_id)
            .map_err(|error| error.to_string())?;
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
}
