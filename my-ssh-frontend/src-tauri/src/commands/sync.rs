use tauri::State;

use crate::state::AppState;
use crate::sync::service::{SyncOperationResult, SyncProvider, SyncService, SyncStatus};

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
pub async fn upload_sync_vault(
    state: State<'_, AppState>,
    token: String,
) -> Result<SyncOperationResult, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .upload(&token)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn change_sync_password(
    state: State<'_, AppState>,
    token: String,
    current_password: String,
    new_password: String,
) -> Result<SyncOperationResult, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .change_password(&token, current_password, new_password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn refresh_sync_derived_key(
    state: State<'_, AppState>,
    token: String,
    sync_password: String,
) -> Result<SyncStatus, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .refresh_derived_key(&token, sync_password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn download_sync_vault(
    state: State<'_, AppState>,
    token: String,
) -> Result<SyncOperationResult, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .download(&token)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resolve_sync_conflict(
    state: State<'_, AppState>,
    token: String,
    resolution: ConflictResolution,
) -> Result<SyncOperationResult, String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    let service = SyncService::new(vault, &state.app_dir).map_err(|error| error.to_string())?;
    match resolution {
        ConflictResolution::KeepLocal => service.resolve_keep_local(&token).await,
        ConflictResolution::AcceptRemote => service.resolve_accept_remote(&token).await,
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
pub async fn delete_remote_sync_vault(
    state: State<'_, AppState>,
    token: String,
) -> Result<(), String> {
    let vault_guard = state.vault.lock().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| "Vault is not open".to_string())?;
    SyncService::new(vault, &state.app_dir)
        .map_err(|error| error.to_string())?
        .delete_remote(&token)
        .await
        .map_err(|error| error.to_string())
}
