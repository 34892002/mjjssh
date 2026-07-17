use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

use crate::state::AppState;

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
