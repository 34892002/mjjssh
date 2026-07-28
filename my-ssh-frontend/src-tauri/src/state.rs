use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::ai::risk_confirmation::RiskConfirmationStore;
use crate::ai::service::{AiTaskManager, SshSafetyContext};
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

pub struct AppState {
    pub vault: Arc<Mutex<Option<Vault>>>,
    pub app_dir: PathBuf,
    pub sessions: Arc<SessionManager>,
    pub local_terminals: Arc<LocalTerminalManager>,
    pub pending_ssh_connections: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub known_hosts: Arc<Mutex<Result<KnownHosts, String>>>,
    pub ai_tasks: AiTaskManager,
    pub risk_confirmations: RiskConfirmationStore,
    pub ssh_safety_contexts: Arc<Mutex<HashMap<String, SshSafetyContext>>>,
    pub server_stats_samples: Mutex<HashMap<String, ServerStatsSample>>,
}

impl AppState {
    pub fn new(app_dir: PathBuf) -> Self {
        Self {
            vault: Arc::new(Mutex::new(None)),
            app_dir: app_dir.clone(),
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

    pub async fn with_vault<F, R>(&self, f: F) -> Result<R, VaultError>
    where
        F: FnOnce(&Vault) -> Result<R, VaultError>,
    {
        let vault_guard = self.vault.lock().await;
        let vault = vault_guard.as_ref().ok_or(VaultError::NotInitialized)?;
        f(vault)
    }
}
