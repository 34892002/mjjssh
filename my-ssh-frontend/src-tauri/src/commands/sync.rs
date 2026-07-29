use tauri::State;

use crate::state::AppState;
use crate::sync::service::{
    RemoteSyncStatus, SyncDiscovery, SyncOperationResult, SyncProvider, SyncService, SyncStatus,
};
use crate::sync::webdav::WebDavCredentials;

#[tauri::command]
pub async fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .status()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn discover_sync_remote(
    state: State<'_, AppState>,
    provider: String,
    token: String,
) -> Result<SyncDiscovery, String> {
    let provider = SyncProvider::parse(&provider).map_err(|error| error.to_string())?;
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .discover(provider, &token)
        .await
        .map_err(|error| error.to_string())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavSyncConfig {
    url: String,
    username: String,
    password: String,
}

impl From<WebDavSyncConfig> for WebDavCredentials {
    fn from(config: WebDavSyncConfig) -> Self {
        Self {
            url: config.url,
            username: config.username,
            password: config.password,
        }
    }
}

#[tauri::command]
pub async fn discover_webdav_sync_remote(
    state: State<'_, AppState>,
    config: WebDavSyncConfig,
) -> Result<SyncDiscovery, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .discover_webdav(config.into())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn enable_webdav_sync(
    state: State<'_, AppState>,
    config: WebDavSyncConfig,
    sync_password: String,
) -> Result<SyncStatus, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .enable_or_import_webdav(config.into(), sync_password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn enable_github_gist_sync(
    state: State<'_, AppState>,
    token: String,
    sync_password: String,
) -> Result<SyncStatus, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    let service = SyncService::new(vault, &state.app_dir).map_err(|error| error.to_string())?;
    service
        .enable_or_import(SyncProvider::GithubGist, &token, sync_password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn enable_gitee_snippet_sync(
    state: State<'_, AppState>,
    token: String,
    sync_password: String,
) -> Result<SyncStatus, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    let service = SyncService::new(vault, &state.app_dir).map_err(|error| error.to_string())?;
    service
        .enable_or_import(SyncProvider::GiteeSnippet, &token, sync_password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn upload_sync_vault(state: State<'_, AppState>) -> Result<SyncOperationResult, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .upload()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn check_remote_sync_status(
    state: State<'_, AppState>,
) -> Result<RemoteSyncStatus, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .check_remote_status()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_auto_sync(
    state: State<'_, AppState>,
    auto_sync: bool,
) -> Result<SyncStatus, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .set_auto_sync(auto_sync)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_local_sync_password(
    state: State<'_, AppState>,
    password: String,
) -> Result<SyncStatus, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .update_local_password(password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn change_sync_password(
    state: State<'_, AppState>,
    current_password: String,
    new_password: String,
) -> Result<SyncOperationResult, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .change_password(current_password, new_password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn download_sync_vault(
    state: State<'_, AppState>,
) -> Result<SyncOperationResult, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .download()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resolve_sync_conflict(
    state: State<'_, AppState>,
    resolution: ConflictResolution,
) -> Result<SyncOperationResult, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    let service = SyncService::new(vault, &state.app_dir).map_err(|error| error.to_string())?;
    match resolution {
        ConflictResolution::KeepLocal => service.resolve_keep_local().await,
        ConflictResolution::AcceptRemote => service.resolve_accept_remote().await,
    }
    .map_err(|error| error.to_string())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    KeepLocal,
    AcceptRemote,
}

#[tauri::command]
pub async fn disable_sync(state: State<'_, AppState>) -> Result<(), String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .disable()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_remote_sync_vault(state: State<'_, AppState>) -> Result<(), String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .delete_remote()
        .await
        .map_err(|error| error.to_string())
}
