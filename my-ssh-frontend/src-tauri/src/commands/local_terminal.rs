use tauri::{AppHandle, State};

use crate::local_terminal::{LocalShellInfo, LocalTerminalManager};
use crate::state::AppState;

#[tauri::command]
pub fn list_local_shells() -> Vec<LocalShellInfo> {
    LocalTerminalManager::available_shells()
}

#[tauri::command]
pub fn start_local_terminal(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    shell: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state
        .local_terminals
        .start(app, session_id, shell, cols, rows)
}

#[tauri::command]
pub fn write_local_terminal_data(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    state.local_terminals.write(&session_id, &data)
}

#[tauri::command]
pub fn resize_local_terminal(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.local_terminals.resize(&session_id, cols, rows)
}

#[tauri::command]
pub fn close_local_terminal(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state.local_terminals.close(&session_id)
}
