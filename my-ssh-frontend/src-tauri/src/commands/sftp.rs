use std::collections::HashMap;
use std::path::Path;
#[cfg(any(windows, unix))]
use std::process::Command;
use std::time::{Instant, SystemTime};

use encoding_rs::{Encoding, GB18030, GBK, UTF_8};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::state::AppState;

const LARGE_TEXT_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TEXT_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TEXT_FILE_BUFFER_BYTES: usize = MAX_TEXT_FILE_BYTES as usize;
const TEXT_READ_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetRemoteTextFileRequest {
    pub session_id: String,
    pub path: String,
    pub allow_large_file: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRemoteTextFileRequest {
    pub session_id: String,
    pub path: String,
    pub content: String,
    pub encoding: RemoteTextEncoding,
    pub line_ending: RemoteLineEnding,
    pub expected_version: RemoteFileVersion,
    pub force: bool,
    pub confirm_binary_write: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteTextEncoding {
    #[serde(rename = "utf-8")]
    Utf8,
    Gbk,
    Gb18030,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteLineEnding {
    Lf,
    Crlf,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileVersion {
    pub size: u64,
    pub modified_at: Option<String>,
    pub content_hash: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileMetadata {
    pub session_id: String,
    pub path: String,
    pub size: u64,
    pub modified_at: Option<String>,
    pub is_symlink: bool,
    pub is_supported_file: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTextFileBytes {
    pub bytes: Vec<u8>,
    pub contains_nul: bool,
    pub version: RemoteFileVersion,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SaveRemoteTextFileResult {
    Saved { version: RemoteFileVersion },
    Conflict { current_version: RemoteFileVersion },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditSession {
    pub edit_id: String,
    pub session_id: String,
    pub path: String,
    pub temp_file_name: String,
    pub local_temp_path: String,
    pub status: ExternalEditSessionState,
    pub version: RemoteFileVersion,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalEditSessionState {
    Clean,
    PendingUpload,
    Uploading,
    Conflict,
    Error,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum UploadExternalEditResult {
    Uploaded { version: RemoteFileVersion },
    Conflict { current_version: RemoteFileVersion },
}

#[derive(Serialize)]
pub struct FileInfo {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    pub mode: u32,
}

#[derive(Serialize)]
pub struct ServerStats {
    pub cpu: String,
    pub memory: String,
    pub disk: String,
    pub net_up: String,
    pub net_down: String,
    pub latency: String,
}

#[derive(Clone, Serialize)]
struct TransferProgress {
    id: String,
    transferred_bytes: u64,
    total_bytes: u64,
}

fn emit_transfer_progress(app: &AppHandle, id: &str, transferred_bytes: u64, total_bytes: u64) {
    let _ = app.emit(
        "sftp-transfer-progress",
        TransferProgress {
            id: id.to_string(),
            transferred_bytes,
            total_bytes,
        },
    );
}

#[tauri::command]
pub async fn open_sftp_window(
    app: AppHandle,
    session_id: String,
    host: String,
    port: u16,
    username: String,
) -> Result<(), String> {
    let label = format!("sftp-{}", session_id);

    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.set_focus();
        return Ok(());
    }

    // 获取父窗口大小（parent() 设置后 position 为相对于父窗口的坐标）
    let parent_label = "main";
    let (width, height) = if let Some(parent) = app.get_webview_window(parent_label) {
        let size = parent.outer_size().unwrap_or_default();
        (size.width, size.height)
    } else {
        (1000u32, 700u32)
    };

    let sftp_width = 500u32;
    let sftp_height = 600u32;

    // 居中在父窗口内
    let sftp_x = ((width as i32 - sftp_width as i32) / 2).max(0);
    let sftp_y = ((height as i32 - sftp_height as i32) / 2).max(0);

    let url = format!(
        "/sftp?sessionId={}&host={}&port={}&username={}",
        session_id, host, port, username
    );

    let mut builder =
        WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.parse().unwrap()))
            .title(format!("SFTP - {}@{}:{}", username, host, port))
            .inner_size(sftp_width as f64, sftp_height as f64)
            .position(sftp_x as f64, sftp_y as f64)
            .decorations(true);

    // 设置父窗口
    if let Some(parent) = app.get_webview_window(parent_label) {
        builder = builder.parent(&parent).map_err(|e| e.to_string())?;
    }

    builder.build().map_err(|e| e.to_string())?;

    Ok(())
}

async fn open_sftp(
    state: &AppState,
    session_id: &str,
) -> Result<std::sync::Arc<SftpSession>, String> {
    state
        .sessions
        .sftp_session(session_id)
        .await
        .map_err(|e| e.to_string())
}

fn validate_remote_text_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        Err("Remote file path must not be empty".to_string())
    } else {
        Ok(())
    }
}

fn metadata_modified_at(metadata: &FileAttributes) -> Option<String> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|time| chrono::DateTime::<chrono::Utc>::from_timestamp(time.as_secs() as i64, 0))
        .map(|time| time.to_rfc3339())
}

async fn preflight_remote_text_file(
    sftp: &SftpSession,
    path: &str,
) -> Result<FileAttributes, String> {
    validate_remote_text_path(path)?;

    // Some SFTP servers close the subsystem when handling lstat requests. Use stat,
    // which is the operation already used by the existing file-management workflow.
    let metadata = sftp
        .metadata(path)
        .await
        .map_err(|error| format!("Unable to inspect remote file: {error}"))?;
    if metadata.is_dir() {
        return Err("Directories are not supported for remote text editing".to_string());
    }
    if !metadata.is_regular() {
        return Err("Only regular files are supported for remote text editing".to_string());
    }
    Ok(metadata)
}

async fn read_remote_text_bytes(sftp: &SftpSession, path: &str) -> Result<Vec<u8>, String> {
    let mut file = sftp
        .open_with_flags(path, OpenFlags::READ)
        .await
        .map_err(|error| format!("Unable to open remote file: {error}"))?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; TEXT_READ_BUFFER_BYTES];

    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("Unable to read remote file: {error}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_TEXT_FILE_BUFFER_BYTES {
            return Err("Remote text file exceeds the 32 MiB editing limit".to_string());
        }
    }
    Ok(bytes)
}

fn version_for(metadata: &FileAttributes, bytes: &[u8]) -> RemoteFileVersion {
    let content_hash = format!("{:x}", Sha256::digest(bytes));
    RemoteFileVersion {
        size: metadata.len(),
        modified_at: metadata_modified_at(metadata),
        content_hash,
    }
}

fn encoding_for(encoding: &RemoteTextEncoding) -> &'static Encoding {
    match encoding {
        RemoteTextEncoding::Utf8 => UTF_8,
        RemoteTextEncoding::Gbk => GBK,
        RemoteTextEncoding::Gb18030 => GB18030,
    }
}

fn encode_text(
    content: &str,
    encoding: &RemoteTextEncoding,
    line_ending: &RemoteLineEnding,
) -> Result<Vec<u8>, String> {
    let line_ending = match line_ending {
        RemoteLineEnding::Lf => "\n",
        RemoteLineEnding::Crlf => "\r\n",
    };
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let content = if line_ending == "\n" {
        normalized
    } else {
        normalized.replace('\n', line_ending)
    };
    let (bytes, _, had_errors) = encoding_for(encoding).encode(&content);
    if had_errors {
        return Err(
            "Text cannot be represented in the selected encoding without replacement".to_string(),
        );
    }
    if bytes.len() > MAX_TEXT_FILE_BUFFER_BYTES {
        return Err("Encoded text exceeds the 32 MiB editing limit".to_string());
    }
    Ok(bytes.into_owned())
}

fn temp_remote_path(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    let directory = path
        .parent()
        .ok_or_else(|| "Invalid remote file path".to_string())?;
    let directory = if directory.as_os_str().is_empty() {
        Path::new(".")
    } else {
        directory
    };
    Ok(directory
        .join(format!(".mjjssh-{}.tmp", Uuid::new_v4()))
        .to_string_lossy()
        .into_owned())
}

async fn cleanup_remote_temp_file(sftp: &SftpSession, path: &str) {
    if let Err(error) = sftp.remove_file(path).await {
        log::warn!("Unable to remove remote text editor temporary file: {error}");
    }
}

async fn overwrite_remote_file(sftp: &SftpSession, path: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = sftp
        .open_with_flags(path, OpenFlags::WRITE | OpenFlags::TRUNCATE)
        .await
        .map_err(|error| format!("Unable to open remote file for overwrite: {error}"))?;
    file.write_all(bytes)
        .await
        .map_err(|error| format!("Unable to overwrite remote file: {error}"))?;
    file.shutdown()
        .await
        .map_err(|error| format!("Unable to close overwritten remote file: {error}"))
}

fn external_edit_session_status(
    record: &crate::state::ExternalEditSessionRecord,
) -> ExternalEditSessionState {
    if record.is_uploading {
        ExternalEditSessionState::Uploading
    } else if record.has_conflict {
        ExternalEditSessionState::Conflict
    } else if record.has_error {
        ExternalEditSessionState::Error
    } else if record.current_hash != record.initial_hash {
        ExternalEditSessionState::PendingUpload
    } else {
        ExternalEditSessionState::Clean
    }
}

fn external_edit_session_response(
    edit_id: String,
    record: &crate::state::ExternalEditSessionRecord,
) -> ExternalEditSession {
    ExternalEditSession {
        edit_id,
        session_id: record.session_id.clone(),
        path: record.remote_path.clone(),
        temp_file_name: record.temp_file_name.clone(),
        local_temp_path: record.temp_path.to_string_lossy().into_owned(),
        status: external_edit_session_status(record),
        version: record.version.clone(),
    }
}

fn external_edit_file_name(path: &str) -> Result<String, String> {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or_else(|| "Remote file path must include a valid file name".to_string())?;
    Ok(file_name.to_string())
}

fn external_edit_session_component(session_id: &str) -> Result<&str, String> {
    if session_id.is_empty()
        || Path::new(session_id).components().count() != 1
        || session_id == "."
        || session_id == ".."
    {
        return Err("Invalid SSH session identifier".to_string());
    }
    Ok(session_id)
}

async fn remove_external_edit_session(state: &AppState, edit_id: &str) -> Result<(), String> {
    let temporary_directory = {
        let sessions = state.external_edit_sessions.lock().await;
        let record = sessions
            .get(edit_id)
            .ok_or_else(|| "External edit session was not found".to_string())?;
        record
            .temp_path
            .parent()
            .ok_or_else(|| "External edit session has an invalid temporary path".to_string())?
            .to_path_buf()
    };
    fs::remove_dir_all(temporary_directory)
        .await
        .map_err(|error| format!("Unable to remove local edit session; close the external application and try again: {error}"))?;
    state.external_edit_sessions.lock().await.remove(edit_id);
    Ok(())
}

#[tauri::command]
pub async fn clear_external_edit_sessions(state: State<'_, AppState>) -> Result<(), String> {
    let temporary_root = state.external_edit_dir.clone();
    match fs::remove_dir_all(&temporary_root).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Unable to clear local edit copies; close external applications and try again: {error}"
            ));
        }
    }
    state.external_edit_sessions.lock().await.clear();
    Ok(())
}

fn edit_local_file(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;

        let operation = std::ffi::OsStr::new("edit")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let file = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result as isize > 32 {
            return Ok(());
        }

        // Many extensions, including .sh, have an `open` association but no `edit` verb.
        // Fall back only to Windows Notepad, never to the `open` association.
        Command::new("notepad.exe")
            .arg(path)
            .spawn()
            .map_err(|error| {
                format!(
                    "Windows could not start an editor for {} (the registered edit action failed with code {} and Notepad could not be started: {})",
                    path.display(),
                    result as isize,
                    error
                )
            })?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-t")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Unable to start the macOS text editor: {error}"))?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let editor = std::env::var_os("VISUAL")
            .or_else(|| std::env::var_os("EDITOR"))
            .ok_or_else(|| "No editor is configured; set VISUAL or EDITOR".to_string())?;
        Command::new(editor)
            .arg(path)
            .spawn()
            .map_err(|error| format!("Unable to start the configured editor: {error}"))?;
        Ok(())
    }

    #[cfg(not(any(windows, target_os = "macos", unix)))]
    {
        let _ = path;
        Err("Default editor support is unavailable on this platform".to_string())
    }
}

async fn stream_remote_to_local(
    sftp: &SftpSession,
    remote_path: &str,
    local_path: &Path,
) -> Result<String, String> {
    let mut remote_file = sftp
        .open_with_flags(remote_path, OpenFlags::READ)
        .await
        .map_err(|error| format!("Unable to open remote file: {error}"))?;
    let mut local_file = fs::File::create(local_path)
        .await
        .map_err(|error| format!("Unable to create local edit file: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; TEXT_READ_BUFFER_BYTES];
    loop {
        let read = remote_file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("Unable to read remote file: {error}"))?;
        if read == 0 {
            break;
        }
        local_file
            .write_all(&buffer[..read])
            .await
            .map_err(|error| format!("Unable to write local edit file: {error}"))?;
        digest.update(&buffer[..read]);
    }
    local_file
        .shutdown()
        .await
        .map_err(|error| format!("Unable to close local edit file: {error}"))?;
    Ok(format!("{:x}", digest.finalize()))
}

async fn hash_local_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).await.map_err(|error| {
        format!(
            "Unable to read local edit file; close the external application and try again: {error}"
        )
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; TEXT_READ_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("Unable to read local edit file; close the external application and try again: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

async fn remote_file_version(sftp: &SftpSession, path: &str) -> Result<RemoteFileVersion, String> {
    let metadata = preflight_remote_text_file(sftp, path).await?;
    let mut file = sftp
        .open_with_flags(path, OpenFlags::READ)
        .await
        .map_err(|error| format!("Unable to open remote file: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; TEXT_READ_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("Unable to read remote file: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(RemoteFileVersion {
        size: metadata.len(),
        modified_at: metadata_modified_at(&metadata),
        content_hash: format!("{:x}", digest.finalize()),
    })
}

async fn stream_local_to_remote(
    local_path: &Path,
    sftp: &SftpSession,
    temporary_path: &str,
) -> Result<String, String> {
    let mut local_file = fs::File::open(local_path).await.map_err(|error| {
        format!(
            "Unable to read local edit file; close the external application and try again: {error}"
        )
    })?;
    let mut remote_file = sftp
        .open_with_flags(
            temporary_path,
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
        )
        .await
        .map_err(|error| format!("Unable to create remote temporary file: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; TEXT_READ_BUFFER_BYTES];
    loop {
        let read = local_file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("Unable to read local edit file; close the external application and try again: {error}"))?;
        if read == 0 {
            break;
        }
        remote_file
            .write_all(&buffer[..read])
            .await
            .map_err(|error| format!("Unable to write remote temporary file: {error}"))?;
        digest.update(&buffer[..read]);
    }
    remote_file
        .shutdown()
        .await
        .map_err(|error| format!("Unable to close remote temporary file: {error}"))?;
    Ok(format!("{:x}", digest.finalize()))
}

#[tauri::command]
pub async fn get_remote_file_metadata(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<RemoteFileMetadata, String> {
    validate_remote_text_path(&path)?;
    let sftp = open_sftp(&state, &session_id).await?;
    let metadata = preflight_remote_text_file(&sftp, &path).await?;
    Ok(RemoteFileMetadata {
        session_id,
        path,
        size: metadata.len(),
        modified_at: metadata_modified_at(&metadata),
        // SFTP stat follows links. Do not issue lstat here because some servers
        // terminate the SFTP subsystem for that request.
        is_symlink: false,
        is_supported_file: true,
    })
}

#[tauri::command]
pub async fn get_remote_text_file(
    state: State<'_, AppState>,
    request: GetRemoteTextFileRequest,
) -> Result<RemoteTextFileBytes, String> {
    validate_remote_text_path(&request.path)?;
    let sftp = open_sftp(&state, &request.session_id).await?;
    let metadata = preflight_remote_text_file(&sftp, &request.path).await?;
    if metadata.len() > LARGE_TEXT_FILE_BYTES && !request.allow_large_file {
        return Err(
            "Remote text file is larger than 2 MiB; confirmation is required before reading it"
                .to_string(),
        );
    }
    if metadata.len() > MAX_TEXT_FILE_BYTES {
        return Err("Remote text file exceeds the 32 MiB editing limit".to_string());
    }

    let bytes = read_remote_text_bytes(&sftp, &request.path).await?;
    Ok(RemoteTextFileBytes {
        contains_nul: bytes.contains(&0),
        version: version_for(&metadata, &bytes),
        bytes,
    })
}

#[tauri::command]
pub async fn save_remote_text_file(
    state: State<'_, AppState>,
    request: SaveRemoteTextFileRequest,
) -> Result<SaveRemoteTextFileResult, String> {
    validate_remote_text_path(&request.path)?;
    let sftp = open_sftp(&state, &request.session_id).await?;
    let metadata = preflight_remote_text_file(&sftp, &request.path).await?;
    if metadata.len() > MAX_TEXT_FILE_BYTES {
        return Err("Remote text file exceeds the 32 MiB editing limit".to_string());
    }
    let current_bytes = read_remote_text_bytes(&sftp, &request.path).await?;
    let current_version = version_for(&metadata, &current_bytes);
    if current_bytes.contains(&0) && !request.confirm_binary_write {
        return Err("Saving will re-encode the entire file and may corrupt binary content; explicit confirmation is required".to_string());
    }
    if !request.force && current_version != request.expected_version {
        return Ok(SaveRemoteTextFileResult::Conflict { current_version });
    }

    let bytes = encode_text(&request.content, &request.encoding, &request.line_ending)?;
    let temporary_path = temp_remote_path(&request.path)?;
    let mut temporary_file = sftp
        .open_with_flags(
            &temporary_path,
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
        )
        .await
        .map_err(|error| format!("Unable to create remote temporary file: {error}"))?;

    let write_result = async {
        temporary_file
            .write_all(&bytes)
            .await
            .map_err(|error| format!("Unable to write remote temporary file: {error}"))?;
        temporary_file
            .shutdown()
            .await
            .map_err(|error| format!("Unable to close remote temporary file: {error}"))
    }
    .await;
    if let Err(error) = write_result {
        cleanup_remote_temp_file(&sftp, &temporary_path).await;
        return Err(error);
    }

    if let Err(rename_error) = sftp.rename(&temporary_path, &request.path).await {
        cleanup_remote_temp_file(&sftp, &temporary_path).await;
        // Some SFTP v3 servers reject rename-overwrite. Preserve conflict detection
        // above, then overwrite the existing file without deleting it as a fallback.
        overwrite_remote_file(&sftp, &request.path, &bytes)
            .await
            .map_err(|overwrite_error| format!(
                "Unable to replace remote file after rename-overwrite was rejected ({rename_error}): {overwrite_error}"
            ))?;
    }

    let saved_metadata = preflight_remote_text_file(&sftp, &request.path).await?;
    let saved_bytes = read_remote_text_bytes(&sftp, &request.path).await?;
    Ok(SaveRemoteTextFileResult::Saved {
        version: version_for(&saved_metadata, &saved_bytes),
    })
}

#[tauri::command]
pub async fn create_external_edit_session(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<ExternalEditSession, String> {
    validate_remote_text_path(&path)?;
    let file_name = external_edit_file_name(&path)?;
    let session_component = external_edit_session_component(&session_id)?;
    let sftp = open_sftp(&state, &session_id).await?;
    let metadata = preflight_remote_text_file(&sftp, &path).await?;

    let edit_id = Uuid::new_v4().to_string();
    let temporary_directory = state
        .external_edit_dir
        .join(session_component)
        .join(&edit_id);
    fs::create_dir_all(&temporary_directory)
        .await
        .map_err(|error| format!("Unable to create local edit directory: {error}"))?;
    let temporary_path = temporary_directory.join(&file_name);

    let initial_hash = match stream_remote_to_local(&sftp, &path, &temporary_path).await {
        Ok(hash) => hash,
        Err(error) => {
            let _ = fs::remove_dir_all(&temporary_directory).await;
            return Err(error);
        }
    };
    let version = RemoteFileVersion {
        size: metadata.len(),
        modified_at: metadata_modified_at(&metadata),
        content_hash: initial_hash.clone(),
    };
    let now = SystemTime::now();
    let record = crate::state::ExternalEditSessionRecord {
        session_id: session_id.clone(),
        remote_path: path.clone(),
        temp_path: temporary_path.clone(),
        temp_file_name: file_name.clone(),
        version: version.clone(),
        initial_hash: initial_hash.clone(),
        current_hash: initial_hash,
        created_at: now,
        last_checked_at: now,
        is_uploading: false,
        has_conflict: false,
        has_error: false,
    };
    state
        .external_edit_sessions
        .lock()
        .await
        .insert(edit_id.clone(), record);
    Ok(ExternalEditSession {
        edit_id,
        session_id,
        path,
        temp_file_name: file_name,
        local_temp_path: temporary_path.to_string_lossy().into_owned(),
        status: ExternalEditSessionState::Clean,
        version,
    })
}

#[tauri::command]
pub async fn edit_external_edit_session(
    state: State<'_, AppState>,
    edit_id: String,
) -> Result<(), String> {
    let temporary_path = {
        let sessions = state.external_edit_sessions.lock().await;
        sessions
            .get(&edit_id)
            .map(|record| record.temp_path.clone())
            .ok_or_else(|| "External edit session was not found".to_string())?
    };
    if !temporary_path.is_file() {
        return Err("The local temporary edit file no longer exists".to_string());
    }

    tokio::task::spawn_blocking(move || edit_local_file(&temporary_path))
        .await
        .map_err(|error| format!("Unable to start the default editor: {error}"))?
}

#[tauri::command]
pub async fn get_external_edit_session_status(
    state: State<'_, AppState>,
    edit_id: String,
) -> Result<ExternalEditSession, String> {
    let temporary_path = {
        let sessions = state.external_edit_sessions.lock().await;
        sessions
            .get(&edit_id)
            .map(|record| record.temp_path.clone())
            .ok_or_else(|| "External edit session was not found".to_string())?
    };

    let hash_result = hash_local_file(&temporary_path).await;
    let mut sessions = state.external_edit_sessions.lock().await;
    let record = sessions
        .get_mut(&edit_id)
        .ok_or_else(|| "External edit session was not found".to_string())?;
    record.last_checked_at = SystemTime::now();
    match hash_result {
        Ok(hash) => {
            record.current_hash = hash;
            record.has_error = false;
        }
        Err(_) => record.has_error = true,
    }
    // Status checks poll because external editors do not provide a portable save-complete
    // callback. They only report a pending upload; they never start one automatically.
    Ok(external_edit_session_response(edit_id, record))
}

#[tauri::command]
pub async fn upload_external_edit_session(
    state: State<'_, AppState>,
    edit_id: String,
    force: bool,
) -> Result<UploadExternalEditResult, String> {
    let (session_id, remote_path, temporary_path, expected_version) = {
        let mut sessions = state.external_edit_sessions.lock().await;
        let record = sessions
            .get_mut(&edit_id)
            .ok_or_else(|| "External edit session was not found".to_string())?;
        if record.is_uploading {
            return Err("External edit session is already uploading".to_string());
        }
        record.is_uploading = true;
        record.has_error = false;
        (
            record.session_id.clone(),
            record.remote_path.clone(),
            record.temp_path.clone(),
            record.version.clone(),
        )
    };

    let result = async {
        let sftp = open_sftp(&state, &session_id).await?;
        let current_version = remote_file_version(&sftp, &remote_path).await?;
        if !force && current_version != expected_version {
            return Ok(UploadExternalEditResult::Conflict { current_version });
        }

        let temporary_remote_path = temp_remote_path(&remote_path)?;
        let local_hash = match stream_local_to_remote(&temporary_path, &sftp, &temporary_remote_path).await {
            Ok(hash) => hash,
            Err(error) => {
                cleanup_remote_temp_file(&sftp, &temporary_remote_path).await;
                return Err(error);
            }
        };
        if let Err(rename_error) = sftp.rename(&temporary_remote_path, &remote_path).await {
            cleanup_remote_temp_file(&sftp, &temporary_remote_path).await;
            let mut local_file = fs::File::open(&temporary_path).await.map_err(|error| {
                format!("Unable to read local edit file for overwrite: {error}")
            })?;
            let mut bytes = Vec::new();
            local_file.read_to_end(&mut bytes).await.map_err(|error| {
                format!("Unable to read local edit file for overwrite: {error}")
            })?;
            overwrite_remote_file(&sftp, &remote_path, &bytes)
                .await
                .map_err(|overwrite_error| format!(
                    "Unable to replace remote file after rename-overwrite was rejected ({rename_error}): {overwrite_error}"
                ))?;
        }

        let saved_version = remote_file_version(&sftp, &remote_path).await?;
        if saved_version.content_hash != local_hash {
            return Err("Remote file changed while confirming the upload".to_string());
        }
        Ok(UploadExternalEditResult::Uploaded { version: saved_version })
    }
    .await;

    let result = match result {
        Ok(UploadExternalEditResult::Uploaded { version }) => {
            if let Err(error) = remove_external_edit_session(&state, &edit_id).await {
                Err(format!(
                    "Remote file was uploaded, but the local temporary copy could not be removed: {error}"
                ))
            } else {
                Ok(UploadExternalEditResult::Uploaded { version })
            }
        }
        result => result,
    };

    let current_hash = match &result {
        Ok(UploadExternalEditResult::Uploaded { .. }) => {
            hash_local_file(&temporary_path).await.ok()
        }
        _ => None,
    };
    let mut sessions = state.external_edit_sessions.lock().await;
    if let Some(record) = sessions.get_mut(&edit_id) {
        record.is_uploading = false;
        record.last_checked_at = SystemTime::now();
        match &result {
            Ok(UploadExternalEditResult::Uploaded { version }) => {
                record.version = version.clone();
                record.initial_hash = version.content_hash.clone();
                record.current_hash = current_hash.unwrap_or_else(|| record.initial_hash.clone());
                record.has_conflict = false;
                record.has_error = false;
            }
            Ok(UploadExternalEditResult::Conflict { .. }) => record.has_conflict = true,
            Err(_) => record.has_error = true,
        }
    }
    result
}

#[tauri::command]
pub async fn reload_external_edit_session(
    state: State<'_, AppState>,
    edit_id: String,
) -> Result<ExternalEditSession, String> {
    let (session_id, remote_path, temporary_path) = {
        let sessions = state.external_edit_sessions.lock().await;
        let record = sessions
            .get(&edit_id)
            .ok_or_else(|| "External edit session was not found".to_string())?;
        if record.is_uploading {
            return Err("External edit session is currently uploading".to_string());
        }
        (
            record.session_id.clone(),
            record.remote_path.clone(),
            record.temp_path.clone(),
        )
    };

    let sftp = open_sftp(&state, &session_id).await?;
    let metadata = preflight_remote_text_file(&sftp, &remote_path).await?;
    let hash = stream_remote_to_local(&sftp, &remote_path, &temporary_path).await?;
    let version = RemoteFileVersion {
        size: metadata.len(),
        modified_at: metadata_modified_at(&metadata),
        content_hash: hash.clone(),
    };

    let mut sessions = state.external_edit_sessions.lock().await;
    let record = sessions
        .get_mut(&edit_id)
        .ok_or_else(|| "External edit session was not found".to_string())?;
    record.version = version;
    record.initial_hash = hash.clone();
    record.current_hash = hash;
    record.last_checked_at = SystemTime::now();
    record.has_conflict = false;
    record.has_error = false;
    Ok(external_edit_session_response(edit_id, record))
}

#[tauri::command]
pub async fn discard_external_edit_session(
    state: State<'_, AppState>,
    edit_id: String,
) -> Result<(), String> {
    remove_external_edit_session(&state, &edit_id).await
}

#[tauri::command]
pub async fn sftp_get_home_directory(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    let output = state
        .sessions
        .execute_command_output(&session_id, "printf '%s' \"$HOME\"".to_string())
        .await
        .map_err(|error| error.to_string())?;
    let home_directory = output.trim();
    if home_directory.starts_with('/') {
        Ok(home_directory.to_string())
    } else {
        Err("Remote home directory is unavailable".to_string())
    }
}

#[tauri::command]
pub async fn sftp_list_files(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<Vec<FileInfo>, String> {
    let sftp = open_sftp(&state, &session_id).await?;
    let entries = sftp.read_dir(&path).await.map_err(|e| e.to_string())?;

    Ok(entries
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name();
            if name == "." || name == ".." {
                return None;
            }
            let metadata = entry.metadata();
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|time| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(time.as_secs() as i64, 0)
                        .map(|date| date.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            Some(FileInfo {
                name,
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified,
                mode: metadata.permissions.unwrap_or(0o755) & 0o777,
            })
        })
        .collect())
}

#[tauri::command]
pub async fn sftp_create_directory(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let sftp = open_sftp(&state, &session_id).await?;
    sftp.create_dir(path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    session_id: String,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    let sftp = open_sftp(&state, &session_id).await?;
    sftp.rename(old_path, new_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sftp_delete(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    is_dir: bool,
) -> Result<(), String> {
    let sftp = open_sftp(&state, &session_id).await?;
    if is_dir {
        sftp.remove_dir(path).await
    } else {
        sftp.remove_file(path).await
    }
    .map_err(|e| e.to_string())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\\"'\\\"'"))
}

#[tauri::command]
pub async fn sftp_set_permissions(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: u32,
) -> Result<(), String> {
    if mode > 0o777 {
        return Err("Invalid permission mode".to_string());
    }
    let command = format!("chmod {:o} -- {}", mode, shell_quote(&path));
    state
        .sessions
        .execute_command(&session_id, command)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sftp_compress_tar_gz(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let path = Path::new(&path);
    let name = path
        .file_name()
        .ok_or_else(|| "Invalid remote path".to_string())?
        .to_string_lossy();
    let parent = path.parent().unwrap_or_else(|| Path::new("/"));
    let archive = format!("{}.tar.gz", name);
    let command = format!(
        "tar -C {} -czf {} -- {}",
        shell_quote(&parent.to_string_lossy()),
        shell_quote(&archive),
        shell_quote(&name)
    );
    state
        .sessions
        .execute_command(&session_id, command)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sftp_extract_tar_gz(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let path = Path::new(&path);
    let parent = path.parent().unwrap_or_else(|| Path::new("/"));
    let archive = path
        .file_name()
        .ok_or_else(|| "Invalid remote path".to_string())?
        .to_string_lossy();
    let command = format!(
        "tar -C {} -xzf {}",
        shell_quote(&parent.to_string_lossy()),
        shell_quote(&archive)
    );
    state
        .sessions
        .execute_command(&session_id, command)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sftp_upload_file(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    local_path: String,
    remote_path: String,
    transfer_id: String,
    overwrite: bool,
) -> Result<(), String> {
    let file_name = Path::new(&local_path)
        .file_name()
        .ok_or_else(|| "Invalid local file path".to_string())?;
    let remote_path = format!(
        "{}/{}",
        remote_path.trim_end_matches('/'),
        file_name.to_string_lossy()
    );
    let local_file = tokio::fs::File::open(&local_path)
        .await
        .map_err(|e| format!("Unable to open local file: {e}"))?;
    let total_bytes = local_file
        .metadata()
        .await
        .map_err(|e| e.to_string())?
        .len();
    let sftp = open_sftp(&state, &session_id).await?;
    let flags = if overwrite {
        OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE
    } else {
        OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE
    };
    let mut remote_file = sftp
        .open_with_flags(&remote_path, flags)
        .await
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(local_file);
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut transferred_bytes = 0_u64;
    emit_transfer_progress(&app, &transfer_id, transferred_bytes, total_bytes);
    loop {
        let read = reader.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        remote_file
            .write_all(&buffer[..read])
            .await
            .map_err(|e| e.to_string())?;
        transferred_bytes += read as u64;
        emit_transfer_progress(&app, &transfer_id, transferred_bytes, total_bytes);
    }
    remote_file.shutdown().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sftp_download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
    local_directory: String,
    transfer_id: String,
    overwrite: bool,
) -> Result<(), String> {
    let file_name = Path::new(&remote_path)
        .file_name()
        .ok_or_else(|| "Invalid remote file path".to_string())?;
    tokio::fs::create_dir_all(&local_directory)
        .await
        .map_err(|e| e.to_string())?;
    let local_path = Path::new(&local_directory).join(file_name);
    let sftp = open_sftp(&state, &session_id).await?;
    let mut remote_file = sftp
        .open_with_flags(&remote_path, OpenFlags::READ)
        .await
        .map_err(|e| e.to_string())?;
    let total_bytes = remote_file
        .metadata()
        .await
        .map_err(|e| e.to_string())?
        .len();
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true);
    if overwrite {
        options.truncate(true);
    } else {
        options.create_new(true);
    }
    let mut local_file = options.open(local_path).await.map_err(|e| e.to_string())?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut transferred_bytes = 0_u64;
    emit_transfer_progress(&app, &transfer_id, transferred_bytes, total_bytes);
    loop {
        let read = remote_file
            .read(&mut buffer)
            .await
            .map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        local_file
            .write_all(&buffer[..read])
            .await
            .map_err(|e| e.to_string())?;
        transferred_bytes += read as u64;
        emit_transfer_progress(&app, &transfer_id, transferred_bytes, total_bytes);
    }
    local_file.shutdown().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn sftp_local_file_exists(local_directory: String, file_name: String) -> bool {
    Path::new(&local_directory).join(file_name).is_file()
}

#[tauri::command]
pub fn get_default_download_directory() -> String {
    std::env::var("USERPROFILE")
        .map(|home| {
            Path::new(&home)
                .join("Downloads")
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_else(|_| ".".to_string())
}

fn parse_stats(output: &str) -> Result<HashMap<&str, u64>, String> {
    output
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            value
                .trim()
                .parse::<u64>()
                .map(|value| (key.trim(), value))
                .map_err(|_| format!("Invalid value for server statistic: {key}"))
        })
        .collect()
}

fn stat_value(stats: &HashMap<&str, u64>, key: &str) -> Result<u64, String> {
    stats
        .get(key)
        .copied()
        .ok_or_else(|| format!("Missing server statistic: {key}"))
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

fn format_gibibytes(bytes: u64) -> String {
    format!("{:.2}G", bytes as f64 / 1024_f64.powi(3))
}

#[tauri::command]
pub async fn get_server_stats(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ServerStats, String> {
    // Collect cumulative counters in a separate SSH exec channel so terminal output is unaffected.
    // Its round-trip time is also a lightweight SSH latency measurement.
    let request_started_at = Instant::now();
    let output = state
        .sessions
        .execute_command_output(
            &session_id,
            "awk '\
/^cpu / { busy=$2+$3+$4+$7+$8+$9; total=busy+$5+$6; print \"cpu_busy=\" busy; print \"cpu_total=\" total } \
/^MemTotal:/ { mem_total=$2 } \
/^MemAvailable:/ { mem_available=$2 } \
END { print \"mem_total=\" mem_total * 1024; print \"mem_used=\" (mem_total-mem_available) * 1024 }' /proc/stat /proc/meminfo; \
df -B1 / | awk 'NR == 2 { print \"disk_used=\" $3; print \"disk_total=\" $2; print \"disk_percent=\" $5 + 0 }'; \
awk -F '[: ]+' 'NR > 2 && $2 != \"lo\" { received += $3; transmitted += $11 } END { print \"net_received=\" received; print \"net_transmitted=\" transmitted }' /proc/net/dev; \
printf 'cpu_cores='; getconf _NPROCESSORS_ONLN"
                .to_string(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let latency_ms = request_started_at.elapsed().as_millis();
    let stats = parse_stats(&output)?;
    let cpu_busy = stat_value(&stats, "cpu_busy")?;
    let cpu_total = stat_value(&stats, "cpu_total")?;
    let net_received = stat_value(&stats, "net_received")?;
    let net_transmitted = stat_value(&stats, "net_transmitted")?;
    let cpu_cores = stat_value(&stats, "cpu_cores")?;
    let now = Instant::now();

    let (cpu_percent, net_down, net_up) = {
        let mut samples = state.server_stats_samples.lock().await;
        let previous = samples.insert(
            session_id,
            crate::state::ServerStatsSample {
                cpu_busy,
                cpu_total,
                net_received,
                net_transmitted,
                captured_at: now,
            },
        );
        match previous {
            Some(previous) if cpu_total > previous.cpu_total => {
                let cpu_percent = (cpu_busy.saturating_sub(previous.cpu_busy) as f64
                    / (cpu_total - previous.cpu_total) as f64
                    * 100.0)
                    .clamp(0.0, 100.0);
                let seconds = now
                    .duration_since(previous.captured_at)
                    .as_secs_f64()
                    .max(0.001);
                let down = net_received.saturating_sub(previous.net_received) as f64 / seconds;
                let up = net_transmitted.saturating_sub(previous.net_transmitted) as f64 / seconds;
                (
                    format!("{cpu_percent:.0}%({cpu_cores}C)"),
                    format!("{}/s", format_bytes(down as u64)),
                    format!("{}/s", format_bytes(up as u64)),
                )
            }
            _ => (
                format!("--({cpu_cores}C)"),
                "--".to_string(),
                "--".to_string(),
            ),
        }
    };

    let memory = format!(
        "{}/{}",
        format_gibibytes(stat_value(&stats, "mem_used")?),
        format_gibibytes(stat_value(&stats, "mem_total")?),
    );
    let disk = format!(
        "{}/{} ({}%)",
        format_bytes(stat_value(&stats, "disk_used")?),
        format_bytes(stat_value(&stats, "disk_total")?),
        stat_value(&stats, "disk_percent")?,
    );

    Ok(ServerStats {
        cpu: cpu_percent,
        memory,
        disk,
        net_up,
        net_down,
        latency: format!("{latency_ms}ms"),
    })
}
