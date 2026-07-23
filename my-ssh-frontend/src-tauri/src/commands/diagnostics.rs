use tauri::State;

use crate::diagnostics;
use crate::state::AppState;

#[tauri::command]
pub async fn record_frontend_crash(
    state: State<'_, AppState>,
    kind: String,
    message: String,
    stack: Option<String>,
) -> Result<(), String> {
    let kind = match kind.as_str() {
        "error" | "unhandled_rejection" => kind,
        _ => return Err("Unsupported frontend crash report type".into()),
    };
    diagnostics::record_frontend_crash(&state.app_dir, &kind, &message, stack.as_deref());
    Ok(())
}

#[tauri::command]
pub async fn export_diagnostic_bundle(state: State<'_, AppState>) -> Result<String, String> {
    diagnostics::export::export_archive(&state)
        .await
        .map(|path| path.display().to_string())
}
