use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::ai::risk_confirmation::RiskConfirmationStore;
use crate::ai::service::{AiTaskManager, SshSafetyContext};
use crate::commands::sftp::RemoteFileVersion;
use crate::local_terminal::LocalTerminalManager;
use crate::ssh::{known_hosts::KnownHosts, SessionManager};
use crate::vault::{Vault, VaultError};

pub struct ServerStatsSample {
    pub cpu_busy: u64,
    pub cpu_total: u64,
    pub net_received: u64,
    pub net_transmitted: u64,
    pub captured_at: Instant,
}

pub struct ExternalEditSessionRecord {
    pub session_id: String,
    pub remote_path: String,
    pub temp_path: PathBuf,
    pub temp_file_name: String,
    pub version: RemoteFileVersion,
    pub initial_hash: String,
    pub current_hash: String,
    pub created_at: SystemTime,
    pub last_checked_at: SystemTime,
    pub is_uploading: bool,
    pub has_conflict: bool,
    pub has_error: bool,
}

pub struct AppState {
    pub vault: Arc<Mutex<Option<Vault>>>,
    pub app_dir: PathBuf,
    pub external_edit_dir: PathBuf,
    pub sessions: Arc<SessionManager>,
    pub local_terminals: Arc<LocalTerminalManager>,
    pub pending_ssh_connections: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub known_hosts: Arc<Mutex<Result<KnownHosts, String>>>,
    pub ai_tasks: AiTaskManager,
    pub risk_confirmations: RiskConfirmationStore,
    pub ssh_safety_contexts: Arc<Mutex<HashMap<String, SshSafetyContext>>>,
    pub server_stats_samples: Mutex<HashMap<String, ServerStatsSample>>,
    pub external_edit_sessions: Mutex<HashMap<String, ExternalEditSessionRecord>>,
    pub minimize_to_tray_on_close: Mutex<bool>,
}

impl AppState {
    pub fn new(app_dir: PathBuf, external_edit_dir: PathBuf) -> Self {
        Self {
            vault: Arc::new(Mutex::new(None)),
            app_dir: app_dir.clone(),
            external_edit_dir,
            sessions: Arc::new(SessionManager::new()),
            local_terminals: Arc::new(LocalTerminalManager::default()),
            pending_ssh_connections: Arc::new(Mutex::new(HashMap::new())),
            known_hosts: Arc::new(Mutex::new(
                KnownHosts::open(app_dir.clone())
                    .map_err(|error| format!("无法读取本地主机信任记录: {error}")),
            )),
            ai_tasks: AiTaskManager::default(),
            risk_confirmations: RiskConfirmationStore::default(),
            ssh_safety_contexts: Arc::new(Mutex::new(HashMap::new())),
            server_stats_samples: Mutex::new(HashMap::new()),
            external_edit_sessions: Mutex::new(HashMap::new()),
            minimize_to_tray_on_close: Mutex::new(false),
        }
    }

    /// 打开或创建本地加密 Vault。日常本地使用不要求用户输入密码。
    pub async fn auto_open(&self) -> Result<(), VaultError> {
        match Vault::open(&self.app_dir) {
            Ok(vault) => {
                *self.vault.lock().await = Some(vault);
                Ok(())
            }
            Err(error) => {
                log::warn!("Could not open local Vault: {}", error);
                Err(error)
            }
        }
    }

    pub async fn is_unlocked(&self) -> bool {
        self.vault.lock().await.is_some()
    }

    pub async fn set_minimize_to_tray_on_close(&self, value: bool) {
        *self.minimize_to_tray_on_close.lock().await = value;
    }

    pub async fn minimize_to_tray_on_close(&self) -> bool {
        *self.minimize_to_tray_on_close.lock().await
    }

    pub async fn with_vault<F, R>(&self, f: F) -> Result<R, VaultError>
    where
        F: FnOnce(&Vault) -> Result<R, VaultError>,
    {
        let vault_guard = self.vault.lock().await;
        let vault = vault_guard.as_ref().ok_or(VaultError::NotInitialized)?;
        f(vault)
    }
}
