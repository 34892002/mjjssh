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

#[tauri::command]
pub fn open_project_repository() -> Result<(), String> {
    const REPOSITORY_URL: &str = "https://github.com/34892002/mjjssh";

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;

        let url: Vec<u16> = std::ffi::OsStr::new(REPOSITORY_URL)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result as isize > 32 {
            return Ok(());
        }
        return Err(format!(
            "Unable to open the project repository (ShellExecuteW returned {result:?})"
        ));
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(REPOSITORY_URL)
            .spawn()
            .map_err(|error| format!("Unable to open the project repository: {error}"))?;
        Ok(())
    }
}
