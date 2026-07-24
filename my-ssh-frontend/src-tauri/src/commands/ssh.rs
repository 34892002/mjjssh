use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

use crate::ai::service::SshSafetyContext;
use crate::ssh::SshSession;
use crate::state::AppState;

const SSH_SAFETY_PROBE: &str = "printf '__MYSSH_SAFETY_CONNECTION__ '; printf '%s %s %s\\n' \"${SSH_CONNECTION:-}\"; printf '__MYSSH_SAFETY_ROUTE__ '; ip route get \"${SSH_CONNECTION%% *}\" 2>/dev/null || true; findmnt -rn -o SOURCE 2>/dev/null | while IFS= read -r source; do printf '__MYSSH_SAFETY_MOUNT__%s\\n' \"$source\"; done; lsblk -rno NAME,PKNAME,MOUNTPOINT 2>/dev/null | while IFS= read -r line; do printf '__MYSSH_SAFETY_BLOCK__%s\\n' \"$line\"; done";

pub const UNRESPONSIVE_TERMINAL_REASON: &str =
    "Interactive terminal did not recover after an interrupted AI command.";

const SSH_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);
const SSH_CONNECTION_CANCELLED_REASON: &str = "SSH connection was cancelled.";

pub async fn disconnect_unresponsive_terminal(
    app: &AppHandle,
    sessions: &Arc<crate::ssh::SessionManager>,
    ai_tasks: &crate::ai::service::AiTaskManager,
    ssh_safety_contexts: &Arc<
        tokio::sync::Mutex<std::collections::HashMap<String, SshSafetyContext>>,
    >,
    session_id: &str,
) {
    log::error!(
        "Interactive SSH terminal recovery failed for session {}; closing the session",
        session_id
    );
    ai_tasks.cancel_tasks_for_session(session_id).await;
    if let Err(error) = sessions.close_session(session_id).await {
        log::warn!(
            "Could not cleanly close unresponsive SSH terminal for session {}: {}",
            session_id,
            error
        );
    }
    ssh_safety_contexts.lock().await.remove(session_id);
    let disconnect_event = format!("ssh-disconnected:{}", session_id);
    if let Err(error) = app.emit(&disconnect_event, UNRESPONSIVE_TERMINAL_REASON) {
        log::warn!(
            "Could not emit unresponsive SSH terminal event for session {}: {}",
            session_id,
            error
        );
    }
}

fn terminal_write_error_category(error: &crate::ssh::SshError) -> &'static str {
    match error {
        crate::ssh::SshError::TerminalBlocked => "terminal_blocked",
        crate::ssh::SshError::TerminalWriterUnavailable => "writer_unavailable",
        crate::ssh::SshError::SessionNotFound(_) => "session_not_found",
        crate::ssh::SshError::Channel(_) => "channel_error",
        crate::ssh::SshError::Connection(_) => "connection_error",
        crate::ssh::SshError::Cancelled => "connection_cancelled",
        crate::ssh::SshError::UnknownHostKey { .. } => "unknown_host_key",
        crate::ssh::SshError::ChangedHostKey { .. } => "changed_host_key",
        crate::ssh::SshError::Auth(_) => "authentication_error",
        crate::ssh::SshError::Io(_) => "io_error",
        crate::ssh::SshError::Ssh(_) => "ssh_error",
    }
}

#[derive(serde::Serialize)]
pub struct SessionInfo {
    pub id: String,
}

#[tauri::command]
pub async fn trust_host_key(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    algorithm: String,
    fingerprint: String,
) -> Result<(), String> {
    if !fingerprint.starts_with("SHA256:") || fingerprint.len() > 128 {
        return Err("无效的主机指纹。".into());
    }
    if algorithm.is_empty() || algorithm.len() > 64 {
        return Err("无效的主机密钥算法。".into());
    }
    state
        .known_hosts
        .lock()
        .await
        .trust(&host, port, algorithm, fingerprint)
        .map_err(|error| format!("无法保存主机指纹: {error}"))
}

#[tauri::command]
pub async fn connect_ssh(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    session_id: String,
) -> Result<String, String> {
    let (profile, credential) = state
        .with_vault(|vault| {
            let profile = vault.get_profile(&profile_id)?;
            let credential = vault.decrypt_credential(&profile)?;
            Ok((profile, credential))
        })
        .await
        .map_err(|e| e.to_string())?;

    let trusted_host_key = state
        .known_hosts
        .lock()
        .await
        .get(&profile.host, profile.port)
        .cloned();
    let expected_host_key = trusted_host_key.map(|trusted_key| crate::ssh::ExpectedHostKey {
        algorithm: trusted_key.algorithm,
        fingerprint: trusted_key.fingerprint,
    });

    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
    let progress_event = format!("ssh-connection-progress:{}", session_id);
    let progress_app = app.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = progress_app.emit(&progress_event, progress);
        }
    });

    let cancellation_token = CancellationToken::new();
    state
        .pending_ssh_connections
        .lock()
        .await
        .insert(session_id.clone(), cancellation_token.clone());

    let connection_result = tokio::select! {
        _ = cancellation_token.cancelled() => Err(SSH_CONNECTION_CANCELLED_REASON.to_owned()),
        result = timeout(
            SSH_CONNECTION_TIMEOUT,
            SshSession::connect(
                session_id.clone(),
                profile_id,
                &profile.host,
                profile.port,
                &profile.username,
                &credential,
                &profile.auth_type,
                expected_host_key,
                progress_tx,
                cancellation_token.clone(),
            ),
        ) => match result {
            Ok(result) => result.map_err(|error| match error {
                crate::ssh::SshError::UnknownHostKey {
                    fingerprint,
                    key_type,
                    ..
                } => format!("HOST_KEY_UNKNOWN|{key_type}|{fingerprint}"),
                crate::ssh::SshError::ChangedHostKey {
                    expected_key_type,
                    expected,
                    actual_key_type,
                    actual,
                    ..
                } => format!("HOST_KEY_CHANGED|{expected_key_type}|{expected}|{actual_key_type}|{actual}"),
                error => error.to_string(),
            }),
            Err(_) => Err(format!("SSH connection timed out after {} seconds.", SSH_CONNECTION_TIMEOUT.as_secs())),
        },
    };

    let (session, mut data_rx) = match connection_result {
        Ok(connection) => connection,
        Err(error) => {
            state
                .pending_ssh_connections
                .lock()
                .await
                .remove(&session_id);
            return Err(error);
        }
    };

    let sessions = state.sessions.clone();
    sessions.add_session(session).await;
    if cancellation_token.is_cancelled() {
        let _ = sessions.close_session(&session_id).await;
        state
            .pending_ssh_connections
            .lock()
            .await
            .remove(&session_id);
        return Err(SSH_CONNECTION_CANCELLED_REASON.to_owned());
    }
    state
        .pending_ssh_connections
        .lock()
        .await
        .remove(&session_id);
    let ai_tasks = state.ai_tasks.clone();
    let ssh_safety_contexts = state.ssh_safety_contexts.clone();

    match sessions
        .execute_command_output(&session_id, SSH_SAFETY_PROBE.into())
        .await
    {
        Ok(output) => {
            state.ssh_safety_contexts.lock().await.insert(
                session_id.clone(),
                SshSafetyContext::from_probe(profile.port, &output),
            );
        }
        Err(error) => log::warn!("Could not collect SSH safety context: {}", error),
    }

    // data_rx 已经从 session 中取出，直接给 spawned task，不需要锁
    let session_id_clone = session_id.clone();
    let app_clone = app.clone();

    tokio::spawn(async move {
        const MAX_BATCH_BYTES: usize = 32 * 1024;
        const MAX_BATCH_WAIT: Duration = Duration::from_millis(8);
        let event_name = format!("ssh-data:{}", session_id_clone);
        let mut batch = Vec::with_capacity(MAX_BATCH_BYTES);

        while let Some(bytes) = data_rx.recv().await {
            batch.extend_from_slice(&bytes);
            while batch.len() < MAX_BATCH_BYTES {
                match timeout(MAX_BATCH_WAIT, data_rx.recv()).await {
                    Ok(Some(bytes)) => batch.extend_from_slice(&bytes),
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            let output = std::mem::take(&mut batch);
            if let Ok(display_output) = sessions
                .filter_terminal_display_output(&session_id_clone, &output)
                .await
            {
                if !display_output.is_empty() {
                    let _ = app_clone.emit(&event_name, display_output);
                }
            }
            sleep(Duration::ZERO).await;
        }

        if !batch.is_empty() {
            if let Ok(display_output) = sessions
                .filter_terminal_display_output(&session_id_clone, &batch)
                .await
            {
                if !display_output.is_empty() {
                    let _ = app_clone.emit(&event_name, display_output);
                }
            }
        }

        if sessions.remove_closed_session(&session_id_clone).await {
            log::warn!(
                "SSH terminal stream closed for session {}",
                session_id_clone
            );
            ai_tasks.cancel_tasks_for_session(&session_id_clone).await;
            ssh_safety_contexts.lock().await.remove(&session_id_clone);
            let disconnect_event = format!("ssh-disconnected:{}", session_id_clone);
            let _ = app_clone.emit(
                &disconnect_event,
                "SSH connection closed by the remote host or network.",
            );
        }
    });

    Ok(session_id)
}

#[tauri::command]
pub async fn disconnect_ssh(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    if let Some(cancellation_token) = state
        .pending_ssh_connections
        .lock()
        .await
        .remove(&session_id)
    {
        cancellation_token.cancel();
    }
    state.ai_tasks.cancel_tasks_for_session(&session_id).await;
    state
        .sessions
        .close_session(&session_id)
        .await
        .map_err(|e| e.to_string())?;
    state.server_stats_samples.lock().await.remove(&session_id);
    state.ssh_safety_contexts.lock().await.remove(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn write_ssh_data(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    match state.sessions.write_to_session(&session_id, &data).await {
        Ok(()) => Ok(()),
        Err(error) => {
            log::warn!(
                "Rejected SSH terminal input for session {}: bytes={}, category={}",
                session_id,
                data.len(),
                terminal_write_error_category(&error)
            );
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub async fn resize_ssh(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state
        .sessions
        .resize_session(&session_id, cols, rows)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<SessionInfo>, String> {
    let session_ids = state.sessions.list_sessions().await;
    Ok(session_ids
        .into_iter()
        .map(|id| SessionInfo { id })
        .collect())
}
