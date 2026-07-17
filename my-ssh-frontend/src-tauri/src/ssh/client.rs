use russh::client::{self, Handler};
use russh::keys::{decode_secret_key, Certificate, PrivateKeyWithHashAlg, PublicKey};
use russh::*;
use russh_sftp::client::SftpSession;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::time::{sleep, timeout, Duration};
use tokio_util::sync::CancellationToken;

use crate::vault::{AuthType, DecryptedCredential};

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("SSH connection failed: {0}")]
    Connection(String),
    #[error("SSH authentication failed: {0}")]
    Auth(String),
    #[error("SSH channel error: {0}")]
    Channel(String),
    #[error("SSH IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SSH error: {0}")]
    Ssh(#[from] russh::Error),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Interactive terminal is unavailable because an SSH write timed out")]
    TerminalBlocked,
    #[error("Interactive terminal writer is unavailable")]
    TerminalWriterUnavailable,
}

#[derive(Debug)]
pub enum InteractiveCommandResult {
    Completed { output: String, exit_code: i32 },
    TimedOut { output: String },
}

fn completion_command(marker: &str) -> String {
    format!("printf '\\r\\033[2K{}:%d__\\r\\033[2K' \"$?\"", marker)
}

fn recovery_command(marker: &str) -> String {
    format!("printf '\\r\\033[2K{}\\r\\033[2K'", marker)
}

fn interactive_input(command: &str, completion_command: &str) -> String {
    format!("{}\r{}\r", command, completion_command)
}

fn completion_status(output: &str, marker: &str) -> Option<(usize, i32)> {
    let mut search_from = 0;
    while let Some(relative_position) = output[search_from..].find(marker) {
        let position = search_from + relative_position;
        let suffix = &output[position + marker.len()..];
        if let Some(status) = suffix
            .strip_prefix(':')
            .and_then(|value| value.split("__").next())
            .and_then(|value| value.trim().parse::<i32>().ok())
        {
            return Some((position, status));
        }
        search_from = position + marker.len();
    }
    None
}

pub struct SshClientHandler {
    data_tx: mpsc::Sender<Vec<u8>>,
    terminal_output_tx: broadcast::Sender<Vec<u8>>,
    terminal_channel_id: Arc<Mutex<Option<ChannelId>>>,
}

const TERMINAL_WRITE_TIMEOUT: Duration = Duration::from_secs(3);
const TERMINAL_PRIORITY_WRITE_TIMEOUT: Duration = Duration::from_millis(750);
const TERMINAL_QUEUE_TIMEOUT: Duration = Duration::from_millis(250);

enum TerminalWriteOperation {
    Data(Vec<u8>),
    PriorityData(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

struct TerminalWriteRequest {
    operation: TerminalWriteOperation,
    cancellation_token: Option<CancellationToken>,
    response: oneshot::Sender<Result<(), SshError>>,
}

async fn run_terminal_writer(
    write_half: ChannelWriteHalf<client::Msg>,
    mut priority_rx: mpsc::Receiver<TerminalWriteRequest>,
    mut normal_rx: mpsc::Receiver<TerminalWriteRequest>,
    terminal_blocked: Arc<AtomicBool>,
) {
    loop {
        let request = tokio::select! {
            biased;
            Some(request) = priority_rx.recv() => request,
            Some(request) = normal_rx.recv() => request,
            else => break,
        };
        // A cancelled caller drops its response receiver. Do not let an AI
        // command that lost its owner execute after cancellation.
        if request.response.is_closed() {
            continue;
        }
        let is_priority = !matches!(request.operation, TerminalWriteOperation::Data(_));
        let write_timeout = if is_priority {
            TERMINAL_PRIORITY_WRITE_TIMEOUT
        } else {
            TERMINAL_WRITE_TIMEOUT
        };
        let operation_name = match &request.operation {
            TerminalWriteOperation::Data(_) | TerminalWriteOperation::PriorityData(_) => "data",
            TerminalWriteOperation::Resize { .. } => "resize",
            TerminalWriteOperation::Close => "close",
        };
        let write = async {
            match request.operation {
                TerminalWriteOperation::Data(data) | TerminalWriteOperation::PriorityData(data) => {
                    write_half.data(&data[..]).await
                }
                TerminalWriteOperation::Resize { cols, rows } => {
                    write_half.window_change(cols, rows, 0, 0).await
                }
                TerminalWriteOperation::Close => write_half.close().await,
            }
        };
        let result = if let Some(cancellation_token) = request.cancellation_token {
            tokio::select! {
                _ = cancellation_token.cancelled() => Err(SshError::Channel("Interactive command cancelled".into())),
                result = timeout(write_timeout, write) => match result {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(error)) => Err(SshError::Channel(error.to_string())),
                    Err(_) => {
                        terminal_blocked.store(true, Ordering::Release);
                        log::error!(
                            "Interactive SSH terminal {} write timed out after {:?}; marking terminal unavailable",
                            operation_name,
                            write_timeout
                        );
                        Err(SshError::TerminalBlocked)
                    }
                },
            }
        } else {
            match timeout(write_timeout, write).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(SshError::Channel(error.to_string())),
                Err(_) => {
                    terminal_blocked.store(true, Ordering::Release);
                    log::error!(
                        "Interactive SSH terminal {} write timed out after {:?}; marking terminal unavailable",
                        operation_name,
                        write_timeout
                    );
                    Err(SshError::TerminalBlocked)
                }
            }
        };
        let _ = request.response.send(result);
    }
}

impl Handler for SshClientHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let is_terminal_channel = self.terminal_channel_id.lock().await.as_ref() == Some(&channel);
        if is_terminal_channel {
            // Completion observers receive data before the lossy WebView relay.
            let _ = self.terminal_output_tx.send(data.to_vec());
            if self.data_tx.try_send(data.to_vec()).is_err() {
                log::warn!("Dropping SSH terminal output because the bounded relay queue is full");
            }
        }
        Ok(())
    }

    async fn extended_data(
        &mut self,
        channel: ChannelId,
        _code: u32,
        data: &[u8],
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let is_terminal_channel = self.terminal_channel_id.lock().await.as_ref() == Some(&channel);
        if is_terminal_channel {
            // Completion observers receive data before the lossy WebView relay.
            let _ = self.terminal_output_tx.send(data.to_vec());
            if self.data_tx.try_send(data.to_vec()).is_err() {
                log::warn!(
                    "Dropping SSH terminal extended output because the bounded relay queue is full"
                );
            }
        }
        Ok(())
    }

    async fn exit_status(
        &mut self,
        _channel: ChannelId,
        exit_status: u32,
        _session: &mut client::Session,
    ) -> Result<(), SshError> {
        log::info!("SSH session exited with status: {}", exit_status);
        Ok(())
    }
}

struct TerminalDisplayFilter {
    patterns: Vec<Vec<u8>>,
    pending: Vec<u8>,
}

impl TerminalDisplayFilter {
    fn hide_once(&mut self, pattern: Vec<u8>) {
        self.patterns.push(pattern);
    }

    fn filter(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        let mut output = Vec::new();

        loop {
            let next_match = self
                .patterns
                .iter()
                .enumerate()
                .filter_map(|(pattern_index, pattern)| {
                    self.pending
                        .windows(pattern.len())
                        .position(|window| window == pattern)
                        .map(|position| (position, pattern_index, pattern.len()))
                })
                .min_by_key(|(position, _, _)| *position);

            let Some((position, pattern_index, pattern_len)) = next_match else {
                break;
            };

            output.extend_from_slice(&self.pending[..position]);
            self.pending.drain(..position + pattern_len);
            self.patterns.remove(pattern_index);
        }

        let keep = self
            .patterns
            .iter()
            .filter_map(|pattern| {
                let max_len = self.pending.len().min(pattern.len());
                (1..=max_len)
                    .rev()
                    .find(|&len| self.pending[self.pending.len() - len..] == pattern[..len])
            })
            .max()
            .unwrap_or(0);
        let emit_len = self.pending.len() - keep;
        output.extend_from_slice(&self.pending[..emit_len]);
        self.pending.drain(..emit_len);
        output
    }
}

pub struct SshSession {
    pub id: String,
    pub server_key: String,
    pub handle: client::Handle<SshClientHandler>,
    terminal_writer_tx: mpsc::Sender<TerminalWriteRequest>,
    terminal_priority_tx: mpsc::Sender<TerminalWriteRequest>,
    terminal_blocked: Arc<AtomicBool>,

    terminal_output_tx: broadcast::Sender<Vec<u8>>,
    terminal_display_filter: Mutex<TerminalDisplayFilter>,
    sftp_session: Mutex<Option<Arc<SftpSession>>>,
}

impl SshSession {
    pub async fn connect(
        session_id: String,
        server_key: String,
        host: &str,
        port: u16,
        username: &str,
        credential: &DecryptedCredential,
        auth_type: &AuthType,
    ) -> Result<(Self, mpsc::Receiver<Vec<u8>>), SshError> {
        let config = client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(300)),
            ..Default::default()
        };

        let config = Arc::new(config);
        // Bound buffered terminal output so a slow WebView cannot grow memory indefinitely.
        let (data_tx, data_rx) = mpsc::channel(128);
        let (terminal_output_tx, _) = broadcast::channel(256);
        let terminal_channel_id = Arc::new(Mutex::new(None));

        let handler = SshClientHandler {
            data_tx,
            terminal_output_tx: terminal_output_tx.clone(),
            terminal_channel_id: terminal_channel_id.clone(),
        };

        let addr = format!("{}:{}", host, port);
        let mut handle = client::connect(config, &addr, handler)
            .await
            .map_err(|e| SshError::Connection(e.to_string()))?;

        match auth_type {
            AuthType::Password => {
                let password = credential
                    .password
                    .as_deref()
                    .ok_or_else(|| SshError::Auth("Password not provided".into()))?;
                handle
                    .authenticate_password(username, password)
                    .await
                    .map_err(|e| SshError::Auth(e.to_string()))?;
            }
            AuthType::Key => {
                let key_pem = credential
                    .private_key
                    .as_deref()
                    .ok_or_else(|| SshError::Auth("Private key not provided".into()))?;
                let key_pair = decode_secret_key(key_pem, None)
                    .map_err(|e| SshError::Auth(format!("Invalid private key: {}", e)))?;
                let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key_pair), None);
                handle
                    .authenticate_publickey(username, key_with_hash)
                    .await
                    .map_err(|e| SshError::Auth(e.to_string()))?;
            }
            AuthType::Certificate => {
                let key_pem = credential
                    .private_key
                    .as_deref()
                    .ok_or_else(|| SshError::Auth("Private key not provided".into()))?;
                let cert_data = credential
                    .cert_data
                    .as_deref()
                    .ok_or_else(|| SshError::Auth("Certificate data not provided".into()))?;

                let key_pair = decode_secret_key(key_pem, None)
                    .map_err(|e| SshError::Auth(format!("Invalid private key: {}", e)))?;

                let cert = Certificate::from_openssh(cert_data)
                    .map_err(|e| SshError::Auth(format!("Invalid certificate: {}", e)))?;

                handle
                    .authenticate_openssh_cert(username, Arc::new(key_pair), cert)
                    .await
                    .map_err(|e| SshError::Auth(e.to_string()))?;
            }
        }

        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;

        channel
            .request_pty(false, "xterm-256color", 80, 24, 0, 0, &[(Pty::ECHO, 1)])
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;

        channel
            .request_shell(true)
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;

        *terminal_channel_id.lock().await = Some(channel.id());
        let (mut read_half, write_half) = channel.split();
        // The handler delivers terminal data. Drain channel messages that are
        // retained for request replies so the split read half cannot back up.
        tokio::spawn(async move { while read_half.wait().await.is_some() {} });
        let (terminal_priority_tx, terminal_priority_rx) = mpsc::channel(16);
        let (terminal_writer_tx, terminal_writer_rx) = mpsc::channel(64);
        let terminal_blocked = Arc::new(AtomicBool::new(false));
        tokio::spawn(run_terminal_writer(
            write_half,
            terminal_priority_rx,
            terminal_writer_rx,
            terminal_blocked.clone(),
        ));

        Ok((
            Self {
                id: session_id,
                server_key,
                handle,
                terminal_writer_tx,
                terminal_priority_tx,
                terminal_blocked,
                terminal_output_tx,
                terminal_display_filter: Mutex::new(TerminalDisplayFilter {
                    patterns: Vec::new(),
                    pending: Vec::new(),
                }),
                sftp_session: Mutex::new(None),
            },
            data_rx,
        ))
    }

    async fn submit_terminal_write(
        &self,
        operation: TerminalWriteOperation,
        priority: bool,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<(), SshError> {
        if !priority && self.terminal_blocked.load(Ordering::Acquire) {
            return Err(SshError::TerminalBlocked);
        }
        let (response_tx, response_rx) = oneshot::channel();
        let request = TerminalWriteRequest {
            operation,
            cancellation_token,
            response: response_tx,
        };
        let sender = if priority {
            &self.terminal_priority_tx
        } else {
            &self.terminal_writer_tx
        };
        timeout(TERMINAL_QUEUE_TIMEOUT, sender.send(request))
            .await
            .map_err(|_| SshError::TerminalWriterUnavailable)?
            .map_err(|_| SshError::TerminalWriterUnavailable)?;
        response_rx
            .await
            .map_err(|_| SshError::TerminalWriterUnavailable)?
    }

    pub async fn write_data(&self, data: &[u8]) -> Result<(), SshError> {
        let priority = data.contains(&b'\x03');
        self.submit_terminal_write(
            if priority {
                TerminalWriteOperation::PriorityData(data.to_vec())
            } else {
                TerminalWriteOperation::Data(data.to_vec())
            },
            priority,
            None,
        )
        .await
    }

    async fn write_ai_command(
        &self,
        data: Vec<u8>,
        cancellation_token: CancellationToken,
    ) -> Result<(), SshError> {
        self.submit_terminal_write(
            TerminalWriteOperation::Data(data),
            false,
            Some(cancellation_token),
        )
        .await
    }

    pub fn interrupt_terminal_best_effort(&self) {
        let sender = self.terminal_priority_tx.clone();
        tokio::spawn(async move {
            let (response_tx, response_rx) = oneshot::channel();
            let request = TerminalWriteRequest {
                operation: TerminalWriteOperation::PriorityData(b"\x03\r".to_vec()),
                cancellation_token: None,
                response: response_tx,
            };
            if timeout(TERMINAL_QUEUE_TIMEOUT, sender.send(request))
                .await
                .is_err()
            {
                log::warn!("Could not queue Ctrl-C for the interactive SSH terminal");
                return;
            }
            if let Err(error) = response_rx.await {
                log::warn!(
                    "Interactive SSH terminal writer stopped before sending Ctrl-C: {}",
                    error
                );
            }
        });
    }

    pub fn subscribe_terminal_output(&self) -> broadcast::Receiver<Vec<u8>> {
        self.terminal_output_tx.subscribe()
    }

    pub async fn filter_terminal_display_output(&self, data: &[u8]) -> Vec<u8> {
        self.terminal_display_filter.lock().await.filter(data)
    }

    pub async fn execute_interactive_command(
        &self,
        command: &str,
        marker: &str,
        cancellation_token: CancellationToken,
    ) -> Result<InteractiveCommandResult, SshError> {
        let mut output = self.subscribe_terminal_output();
        let completion_command = completion_command(marker);
        self.terminal_display_filter
            .lock()
            .await
            .hide_once(completion_command.as_bytes().to_vec());
        // Match xterm's Enter key. Interactive PTYs expect carriage return;
        // using it consistently also prevents a shell from retaining a partial
        // line after an AI command times out.
        let input = interactive_input(command, &completion_command);
        log::info!(
            "Submitting interactive SSH command and waiting for completion marker {}",
            marker
        );
        match self
            .write_ai_command(input.into_bytes(), cancellation_token.clone())
            .await
        {
            Ok(()) => {}
            Err(SshError::Channel(message)) if message == "Interactive command cancelled" => {
                self.interrupt_terminal_best_effort();
                return Err(SshError::Channel(message));
            }
            Err(error) => return Err(error),
        }

        let mut collected = String::new();
        let completion = timeout(std::time::Duration::from_secs(60), async {
            loop {
                match output.recv().await {
                    Ok(bytes) => {
                        collected.push_str(&String::from_utf8_lossy(&bytes));
                        if let Some((position, status)) = completion_status(&collected, marker) {
                            log::info!(
                                "Observed interactive SSH completion marker {} with exit status {}",
                                marker,
                                status
                            );
                            return Ok((collected[..position].to_owned(), status));
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!(
                            "Lost {} terminal output batches while waiting for completion marker {}",
                            skipped,
                            marker
                        );
                        return Err(SshError::Channel(
                            "Terminal output was dropped before command completion could be confirmed".into(),
                        ));
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(SshError::Channel("Terminal output closed".into()))
                    }
                }
            }
        });
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                // Cancellation must release the AI task even when SSH flow control
                // prevents a Ctrl-C from reaching the remote PTY.
                self.interrupt_terminal_best_effort();
                Err(SshError::Channel("Interactive command cancelled".into()))
            }
            result = completion => match result {
                Ok(Ok((output, exit_code))) => Ok(InteractiveCommandResult::Completed { output, exit_code }),
                Ok(Err(error)) => Err(error),
                Err(_) => {
                    log::warn!(
                        "Timed out waiting for interactive SSH completion marker {}; sending Ctrl-C",
                        marker
                    );
                    self.interrupt_terminal_best_effort();
                    Ok(InteractiveCommandResult::TimedOut { output: collected })
                }
            },
        }
    }

    pub async fn recover_interactive_terminal(
        &self,
        marker: &str,
        cancellation_token: CancellationToken,
    ) -> Result<bool, SshError> {
        const RECOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

        let mut output = self.subscribe_terminal_output();
        let recovery_command = recovery_command(marker);
        self.terminal_display_filter
            .lock()
            .await
            .hide_once(recovery_command.as_bytes().to_vec());
        // Let the remote PTY process Ctrl-C before sending a new shell command.
        // Without this short delay, the recovery marker can be consumed by the
        // interrupted program instead of the interactive shell.
        sleep(std::time::Duration::from_millis(200)).await;
        log::info!(
            "Sending interactive SSH recovery marker {} after command interruption",
            marker
        );
        self.submit_terminal_write(
            TerminalWriteOperation::PriorityData(format!("{}\r", recovery_command).into_bytes()),
            true,
            None,
        )
        .await?;

        let recovery = timeout(RECOVERY_TIMEOUT, async {
            let mut collected = String::new();
            loop {
                match output.recv().await {
                    Ok(bytes) => {
                        collected.push_str(&String::from_utf8_lossy(&bytes));
                        if collected.contains(marker) {
                            log::info!("Observed interactive SSH recovery marker {}", marker);
                            return Ok(true);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!(
                            "Lost {} terminal output batches while waiting for recovery marker {}",
                            skipped,
                            marker
                        );
                        return Ok(false);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(SshError::Channel("Terminal output closed".into()))
                    }
                }
            }
        });
        tokio::select! {
            _ = cancellation_token.cancelled() => Err(SshError::Channel("Interactive command cancelled".into())),
            result = recovery => match result {
                Ok(result) => result,
                Err(_) => {
                    log::warn!(
                        "Timed out waiting for interactive SSH recovery marker {}",
                        marker
                    );
                    Ok(false)
                }
            },
        }
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), SshError> {
        self.submit_terminal_write(TerminalWriteOperation::Resize { cols, rows }, true, None)
            .await
    }

    pub async fn sftp_session(&self) -> Result<Arc<SftpSession>, SshError> {
        let mut cached_session = self.sftp_session.lock().await;
        if let Some(session) = cached_session.as_ref() {
            return Ok(session.clone());
        }

        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;
        let session = Arc::new(
            SftpSession::new(channel.into_stream())
                .await
                .map_err(|e| SshError::Channel(e.to_string()))?,
        );
        *cached_session = Some(session.clone());
        Ok(session)
    }

    pub async fn execute_command_output(&self, command: String) -> Result<String, SshError> {
        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;
        channel
            .exec(true, command)
            .await
            .map_err(|e| SshError::Channel(e.to_string()))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus {
                    exit_status: status,
                } => exit_status = Some(status),
                _ => {}
            }
        }
        match exit_status {
            Some(0) => Ok(String::from_utf8_lossy(&stdout).into_owned()),
            Some(status) => Err(SshError::Channel(format!(
                "Remote command failed with status {}: {}",
                status,
                String::from_utf8_lossy(&stderr)
            ))),
            None => Err(SshError::Channel(
                "Remote command did not return an exit status".into(),
            )),
        }
    }

    pub async fn execute_command(&self, command: String) -> Result<(), SshError> {
        self.execute_command_output(command).await.map(|_| ())
    }

    pub async fn close(&self) -> Result<(), SshError> {
        if let Some(sftp_session) = self.sftp_session.lock().await.take() {
            sftp_session
                .close()
                .await
                .map_err(|e| SshError::Channel(e.to_string()))?;
        }
        if let Err(error) = self
            .submit_terminal_write(TerminalWriteOperation::Close, true, None)
            .await
        {
            log::warn!("Could not close interactive SSH channel cleanly: {}", error);
        }
        self.handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await
            .map_err(|e| SshError::Channel(e.to_string()))
    }
}

pub struct SessionManager {
    pub sessions: Mutex<HashMap<String, Arc<SshSession>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub async fn add_session(&self, session: SshSession) {
        let id = session.id.clone();
        self.sessions.lock().await.insert(id, Arc::new(session));
    }

    async fn get_session(&self, id: &str) -> Result<Arc<SshSession>, SshError> {
        self.sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| SshError::SessionNotFound(id.to_string()))
    }

    pub async fn write_to_session(&self, id: &str, data: &[u8]) -> Result<(), SshError> {
        self.get_session(id).await?.write_data(data).await
    }

    pub async fn resize_session(&self, id: &str, cols: u16, rows: u16) -> Result<(), SshError> {
        self.get_session(id)
            .await?
            .resize(cols as u32, rows as u32)
            .await
    }

    pub async fn sftp_session(&self, id: &str) -> Result<Arc<SftpSession>, SshError> {
        self.get_session(id).await?.sftp_session().await
    }

    pub async fn execute_command(&self, id: &str, command: String) -> Result<(), SshError> {
        self.get_session(id).await?.execute_command(command).await
    }

    pub async fn execute_interactive_command(
        &self,
        id: &str,
        command: &str,
        marker: &str,
        cancellation_token: CancellationToken,
    ) -> Result<InteractiveCommandResult, SshError> {
        self.get_session(id)
            .await?
            .execute_interactive_command(command, marker, cancellation_token)
            .await
    }

    pub async fn recover_interactive_terminal(
        &self,
        id: &str,
        marker: &str,
        cancellation_token: CancellationToken,
    ) -> Result<bool, SshError> {
        self.get_session(id)
            .await?
            .recover_interactive_terminal(marker, cancellation_token)
            .await
    }

    pub async fn filter_terminal_display_output(
        &self,
        id: &str,
        data: &[u8],
    ) -> Result<Vec<u8>, SshError> {
        Ok(self
            .get_session(id)
            .await?
            .filter_terminal_display_output(data)
            .await)
    }

    pub async fn server_key(&self, id: &str) -> Result<String, SshError> {
        Ok(self.get_session(id).await?.server_key.clone())
    }

    pub async fn execute_command_output(
        &self,
        id: &str,
        command: String,
    ) -> Result<String, SshError> {
        self.get_session(id)
            .await?
            .execute_command_output(command)
            .await
    }

    pub async fn close_session(&self, id: &str) -> Result<(), SshError> {
        let session = self.sessions.lock().await.remove(id);
        if let Some(session) = session {
            session.close().await?;
        }
        Ok(())
    }

    /// Removes a session after the remote terminal stream has closed. This does
    /// not attempt another write or SSH disconnect on an already closed path.
    pub async fn remove_closed_session(&self, id: &str) -> bool {
        self.sessions.lock().await.remove(id).is_some()
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        self.sessions.lock().await.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        completion_command, completion_status, interactive_input, recovery_command,
        TerminalDisplayFilter,
    };

    #[test]
    fn completion_command_keeps_terminal_controls_as_bash_escapes() {
        let command = completion_command("__MYSSH_AI_DONE_test__");

        assert_eq!(
            command,
            "printf '\\r\\033[2K__MYSSH_AI_DONE_test__:%d__\\r\\033[2K' \"$?\""
        );
        assert!(!command.contains('\r'));
        assert!(!command.contains('\u{1b}'));
    }

    #[test]
    fn recovery_command_keeps_terminal_controls_as_bash_escapes() {
        let command = recovery_command("__MYSSH_AI_RECOVERED_test__");

        assert_eq!(
            command,
            "printf '\\r\\033[2K__MYSSH_AI_RECOVERED_test__\\r\\033[2K'"
        );
        assert!(!command.contains('\r'));
        assert!(!command.contains('\u{1b}'));
    }

    #[test]
    fn interactive_commands_use_carriage_return_like_xterm_enter() {
        assert_eq!(
            interactive_input("echo diagnostic", "printf marker"),
            "echo diagnostic\rprintf marker\r"
        );
    }

    #[test]
    fn completion_status_detects_a_marker_split_across_terminal_output() {
        let marker = "__MYSSH_AI_DONE_abc__";
        let mut output = String::from("command output\\n__MYSSH_AI_DONE_");
        output.push_str("abc__:0__");

        assert_eq!(
            completion_status(&output, marker),
            Some(("command output\\n".len(), 0))
        );
    }

    #[test]
    fn terminal_display_filter_hides_completion_command_split_across_chunks() {
        let command = completion_command("__MYSSH_AI_DONE_test__");
        let split_at = command.len() / 2;
        let mut filter = TerminalDisplayFilter {
            patterns: vec![command.as_bytes().to_vec()],
            pending: Vec::new(),
        };

        let first_chunk = format!("before {command}");
        let first = filter.filter(&first_chunk.as_bytes()[.."before ".len() + split_at]);
        let second_chunk = format!("before {command} after");
        let second = filter.filter(&second_chunk.as_bytes()["before ".len() + split_at..]);

        assert_eq!(String::from_utf8(first).unwrap(), "before ");
        assert_eq!(String::from_utf8(second).unwrap(), " after");
    }

    #[test]
    fn completion_status_ignores_incomplete_markers() {
        let marker = "__MYSSH_AI_DONE_abc__";
        let output = format!("before {marker}:not-a-status__ after {marker}:7__");

        assert_eq!(
            completion_status(&output, marker),
            Some((output.rfind(marker).unwrap(), 7))
        );
    }

    #[test]
    fn completion_status_requires_a_complete_exit_code() {
        let marker = "__MYSSH_AI_DONE_abc__";

        assert_eq!(completion_status(&format!("{marker}:"), marker), None);
    }
}
