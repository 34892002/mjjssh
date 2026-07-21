use std::time::Duration;

use serde::Deserialize;
use tauri::State;
use tokio::time::timeout;

use crate::ssh::SshSession;
use crate::state::AppState;
use crate::vault::{
    CreateKeyRequest, CreateProfileRequest, SshKeyView, SshProfileView, UpdateProfileRequest,
};

const PROFILE_INFO_COMMAND: &str = "printf '__MYSSH_OS__'; if [ -r /etc/os-release ]; then . /etc/os-release; printf '%s' \"${PRETTY_NAME:-${NAME:-unknown}}\"; else uname -srm; fi; printf '\\n__MYSSH_IPINFO__'; if command -v curl >/dev/null 2>&1; then curl --fail --silent --show-error --max-time 5 https://ipinfo.io/json; elif command -v wget >/dev/null 2>&1; then wget -qO- --timeout=5 https://ipinfo.io/json; fi";

#[derive(Deserialize)]
struct IpInfoResponse {
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
}

fn profile_location(ip_info: &IpInfoResponse) -> Option<String> {
    let location = [
        ip_info.country.as_deref(),
        ip_info.region.as_deref(),
        ip_info.city.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .map(str::trim)
    .collect::<Vec<_>>()
    .join(", ");
    (!location.is_empty()).then_some(location)
}

fn parse_profile_info(output: &str) -> Result<(String, Option<String>), String> {
    let (_, output) = output
        .split_once("__MYSSH_OS__")
        .ok_or_else(|| "系统信息命令未返回 OS 标记".to_string())?;
    let (os, ip_info) = output
        .split_once("\n__MYSSH_IPINFO__")
        .ok_or_else(|| "系统信息命令未返回网络信息标记".to_string())?;
    let os = os.trim();
    if os.is_empty() {
        return Err("未能读取远端操作系统信息".into());
    }
    let location = serde_json::from_str::<IpInfoResponse>(ip_info.trim())
        .ok()
        .and_then(|info| profile_location(&info));
    Ok((os.to_owned(), location))
}

/// 初始化本地 JSON Vault；文件不存在时会创建空 Vault。
#[tauri::command]
pub async fn init_vault(state: State<'_, AppState>) -> Result<(), String> {
    if state.is_unlocked().await {
        return Ok(());
    }
    state.auto_open().await.map_err(|error| error.to_string())
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

#[tauri::command]
pub async fn refresh_profile_info(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<SshProfileView, String> {
    let (profile, credential) = state
        .with_vault(|vault| {
            let profile = vault.get_profile(&profile_id)?;
            let credential = vault.decrypt_credential(&profile)?;
            Ok((profile, credential))
        })
        .await
        .map_err(|error| error.to_string())?;

    let temporary_session_id = format!("profile-info-{}", uuid::Uuid::new_v4());
    let (session, _data_rx) = timeout(
        Duration::from_secs(15),
        SshSession::connect(
            temporary_session_id,
            profile.id.clone(),
            &profile.host,
            profile.port,
            &profile.username,
            &credential,
            &profile.auth_type,
        ),
    )
    .await
    .map_err(|_| "连接主机以更新信息超时".to_string())?
    .map_err(|error| error.to_string())?;

    let output = timeout(
        Duration::from_secs(12),
        session.execute_command_output(PROFILE_INFO_COMMAND.to_owned()),
    )
    .await
    .map_err(|_| "读取主机信息超时".to_string())?
    .map_err(|error| error.to_string());
    if let Err(error) = session.close().await {
        log::warn!("Could not close temporary profile info session: {}", error);
    }
    let output = output?;
    let (os, location) = parse_profile_info(&output)?;

    state
        .with_vault(|vault| {
            let updated = vault.update_profile(
                &profile_id,
                &UpdateProfileRequest {
                    os: Some(os),
                    location,
                    name: None,
                    host: None,
                    port: None,
                    username: None,
                    auth_type: None,
                    credential: None,
                    key_id: None,
                    group_name: None,
                    icon: None,
                    color: None,
                },
            )?;
            Ok(SshProfileView::from(&updated))
        })
        .await
        .map_err(|error| error.to_string())
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
