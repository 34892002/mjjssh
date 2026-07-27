use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter};

pub const POWERSHELL: &str = "powershell";
pub const GIT_BASH: &str = "git-bash";

const GIT_BASH_PATHS: [&str; 2] = [
    r"C:\Program Files\Git\bin\bash.exe",
    r"C:\Program Files (x86)\Git\bin\bash.exe",
];

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellInfo {
    pub id: String,
    pub label: String,
}

struct LocalTerminalSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

#[derive(Default)]
pub struct LocalTerminalManager {
    sessions: Arc<Mutex<HashMap<String, LocalTerminalSession>>>,
}

impl LocalTerminalManager {
    pub fn available_shells() -> Vec<LocalShellInfo> {
        let mut shells = vec![LocalShellInfo {
            id: POWERSHELL.into(),
            label: "PowerShell".into(),
        }];
        if git_bash_path().is_some() {
            shells.push(LocalShellInfo {
                id: GIT_BASH.into(),
                label: "Git Bash".into(),
            });
        }
        shells
    }

    pub fn start(
        &self,
        app: AppHandle,
        session_id: String,
        shell: String,
        cols: u16,
        rows: u16,
    ) -> Result<(), String> {
        let command = shell_command(&shell)?;
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("无法创建本地终端: {error}"))?;
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("无法启动本地终端: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("无法打开本地终端输入: {error}"))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("无法打开本地终端输出: {error}"))?;

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "本地终端状态不可用".to_owned())?;
        if sessions.contains_key(&session_id) {
            return Err("本地终端已存在".into());
        }
        sessions.insert(
            session_id.clone(),
            LocalTerminalSession {
                master: pair.master,
                writer,
                child,
            },
        );
        drop(sessions);

        let data_event = format!("local-terminal-data:{session_id}");
        let closed_event = format!("local-terminal-closed:{session_id}");
        let sessions = self.sessions.clone();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if app.emit(&data_event, buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            if let Ok(mut sessions) = sessions.lock() {
                sessions.remove(&session_id);
            }
            let _ = app.emit(&closed_event, "本地终端已退出");
        });
        Ok(())
    }

    pub fn write(&self, session_id: &str, data: &[u8]) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "本地终端状态不可用".to_owned())?;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| "本地终端不存在".to_owned())?;
        session
            .writer
            .write_all(data)
            .and_then(|_| session.writer.flush())
            .map_err(|error| format!("无法写入本地终端: {error}"))
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| "本地终端状态不可用".to_owned())?;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| "本地终端不存在".to_owned())?;
        session
            .master
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("无法调整本地终端大小: {error}"))
    }

    pub fn close(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "本地终端状态不可用".to_owned())?;
        let mut session = sessions
            .remove(session_id)
            .ok_or_else(|| "本地终端不存在".to_owned())?;
        session
            .child
            .kill()
            .map_err(|error| format!("无法关闭本地终端: {error}"))
    }
}

fn shell_command(shell: &str) -> Result<CommandBuilder, String> {
    match shell {
        POWERSHELL => Ok(CommandBuilder::new("powershell.exe")),
        GIT_BASH => git_bash_path()
            .map(CommandBuilder::new)
            .ok_or_else(|| "未检测到 Git Bash".to_owned()),
        _ => Err("不支持的本地终端".into()),
    }
}

fn git_bash_path() -> Option<&'static str> {
    GIT_BASH_PATHS
        .iter()
        .copied()
        .find(|path| Path::new(path).is_file())
}
