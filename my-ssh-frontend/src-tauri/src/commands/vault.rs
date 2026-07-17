use tauri::State;

use crate::state::AppState;
use crate::vault::{
    CreateKeyRequest, CreateProfileRequest, SshKeyView, SshProfileView, UpdateProfileRequest,
};

/// 初始化 vault（尝试自动打开）
#[tauri::command]
pub async fn init_vault(state: State<'_, AppState>) -> Result<bool, String> {
    if state.is_unlocked().await {
        return Ok(true);
    }
    match state.auto_open().await {
        Ok(()) => Ok(true),
        Err(crate::vault::VaultError::NotInitialized) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

/// 首次设置：用主密码初始化 vault
#[tauri::command]
pub async fn setup_vault(state: State<'_, AppState>, password: String) -> Result<(), String> {
    state.setup(&password).await.map_err(|e| e.to_string())
}

/// 修改主密码
#[tauri::command]
pub async fn change_password(
    state: State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> Result<(), String> {
    state
        .change_password(&old_password, &new_password)
        .await
        .map_err(|e| e.to_string())
}

/// 检查是否使用默认密码
#[tauri::command]
pub async fn is_default_password(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.is_default_password().await)
}

#[tauri::command]
pub async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<SshProfileView>, String> {
    state
        .with_vault(|vault| {
            let profiles = vault.list_profiles()?;
            Ok(profiles.iter().map(SshProfileView::from).collect())
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_profile(state: State<'_, AppState>, id: String) -> Result<SshProfileView, String> {
    state
        .with_vault(|vault| {
            let profile = vault.get_profile(&id)?;
            Ok(SshProfileView::from(&profile))
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_profile(
    state: State<'_, AppState>,
    profile: CreateProfileRequest,
) -> Result<SshProfileView, String> {
    state
        .with_vault(|vault| {
            let created = vault.create_profile(&profile)?;
            Ok(SshProfileView::from(&created))
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_profile(
    state: State<'_, AppState>,
    id: String,
    profile: UpdateProfileRequest,
) -> Result<SshProfileView, String> {
    state
        .with_vault(|vault| {
            let updated = vault.update_profile(&id, &profile)?;
            Ok(SshProfileView::from(&updated))
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .with_vault(|vault| vault.delete_profile(&id))
        .await
        .map_err(|e| e.to_string())
}

// ==================== SSH Keys ====================

#[tauri::command]
pub async fn list_keys(state: State<'_, AppState>) -> Result<Vec<SshKeyView>, String> {
    state
        .with_vault(|vault| vault.list_keys())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_key(
    state: State<'_, AppState>,
    key: CreateKeyRequest,
) -> Result<SshKeyView, String> {
    state
        .with_vault(|vault| vault.create_key(&key))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_key(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .with_vault(|vault| vault.delete_key(&id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_key(
    state: State<'_, AppState>,
    id: String,
    key: CreateKeyRequest,
) -> Result<SshKeyView, String> {
    state
        .with_vault(|vault| vault.update_key(&id, &key))
        .await
        .map_err(|e| e.to_string())
}
