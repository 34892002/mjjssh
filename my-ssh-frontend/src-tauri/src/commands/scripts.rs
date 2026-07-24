use tauri::State;

use crate::state::AppState;
use crate::vault::{CreateScriptRequest, Script, UpdateScriptRequest};

#[tauri::command]
pub async fn list_scripts(state: State<'_, AppState>) -> Result<Vec<Script>, String> {
    state
        .with_vault(|vault| vault.list_scripts())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_script(state: State<'_, AppState>, id: String) -> Result<Script, String> {
    state
        .with_vault(|vault| vault.get_script(&id))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_script(
    state: State<'_, AppState>,
    script: CreateScriptRequest,
) -> Result<Script, String> {
    state
        .with_vault(|vault| vault.create_script(&script))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_script(
    state: State<'_, AppState>,
    id: String,
    script: UpdateScriptRequest,
) -> Result<Script, String> {
    state
        .with_vault(|vault| vault.update_script(&id, &script))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_script(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .with_vault(|vault| vault.delete_script(&id))
        .await
        .map_err(|error| error.to_string())
}
