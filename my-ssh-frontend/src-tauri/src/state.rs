use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::ai::risk_confirmation::RiskConfirmationStore;
use crate::ai::service::{AiTaskManager, SshSafetyContext};
use crate::ssh::SessionManager;
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
    pub ai_tasks: AiTaskManager,
    pub risk_confirmations: RiskConfirmationStore,
    pub ssh_safety_contexts: Arc<Mutex<HashMap<String, SshSafetyContext>>>,
    pub server_stats_samples: Mutex<HashMap<String, ServerStatsSample>>,
}

impl AppState {
    pub fn new(app_dir: PathBuf) -> Self {
        Self {
            vault: Arc::new(Mutex::new(None)),
            app_dir,
            sessions: Arc::new(SessionManager::new()),
            ai_tasks: AiTaskManager::default(),
            risk_confirmations: RiskConfirmationStore::default(),
            ssh_safety_contexts: Arc::new(Mutex::new(HashMap::new())),
            server_stats_samples: Mutex::new(HashMap::new()),
        }
    }

    /// 自动打开 vault（读取 local.key）
    pub async fn auto_open(&self) -> Result<(), VaultError> {
        let vault = Vault::open_auto(&self.app_dir)?;
        *self.vault.lock().await = Some(vault);
        Ok(())
    }

    /// 首次设置：用主密码初始化 vault
    pub async fn setup(&self, password: &str) -> Result<(), VaultError> {
        let vault = Vault::setup(&self.app_dir, password)?;
        *self.vault.lock().await = Some(vault);
        Ok(())
    }

    /// 修改主密码
    pub async fn change_password(
        &self,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), VaultError> {
        Vault::change_password(&self.app_dir, old_password, new_password)?;
        // 用新密码重新打开 vault
        let vault = Vault::open_auto(&self.app_dir)?;
        *self.vault.lock().await = Some(vault);
        Ok(())
    }

    /// 检查是否使用默认密码
    pub async fn is_default_password(&self) -> bool {
        Vault::is_default_password(&self.app_dir)
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
