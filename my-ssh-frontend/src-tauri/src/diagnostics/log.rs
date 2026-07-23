use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MAX_LOG_BYTES: u64 = 512 * 1024;
const ROTATED_LOGS: usize = 2;

static DIAGNOSTICS_LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn initialize(path: PathBuf) {
    let _ = DIAGNOSTICS_LOG_PATH.set(path);
}

pub fn event(component: &str, event: &str, detail: impl AsRef<str>) {
    let detail = sanitize_detail(detail.as_ref());
    write(&format!(
        "component={component} event={event} detail={detail:?}"
    ));
}

pub fn error(component: &str, category: &str) {
    write(&format!(
        "component={component} event=error category={category}"
    ));
}

fn write(message: &str) {
    let Some(path) = DIAGNOSTICS_LOG_PATH.get() else {
        return;
    };
    let Ok(_lock) = WRITE_LOCK.lock() else {
        return;
    };
    if rotate_if_needed(path).is_err() {
        return;
    }
    let line = format!(
        "{} {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        message
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

fn sanitize_detail(detail: &str) -> String {
    let trimmed = detail.trim();
    if trimmed.is_empty() {
        return "none".into();
    }
    let has_sensitive_marker = [
        "password",
        "token",
        "api_key",
        "authorization",
        "private key",
        "begin ",
        "ssh-rsa",
        "ssh-ed25519",
        "bearer ",
    ]
    .iter()
    .any(|marker| trimmed.to_ascii_lowercase().contains(marker));
    if has_sensitive_marker {
        return "redacted".into();
    }
    trimmed.chars().take(160).collect()
}

fn rotate_if_needed(path: &Path) -> std::io::Result<()> {
    if fs::metadata(path).map_or(0, |metadata| metadata.len()) < MAX_LOG_BYTES {
        return Ok(());
    }
    for index in (1..=ROTATED_LOGS).rev() {
        let source = rotated_path(path, index);
        let destination = rotated_path(path, index + 1);
        if index == ROTATED_LOGS {
            let _ = fs::remove_file(&source);
        } else if source.exists() {
            let _ = fs::remove_file(&destination);
            fs::rename(source, destination)?;
        }
    }
    fs::rename(path, rotated_path(path, 1))
}

pub fn rotated_path(path: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}

#[cfg(test)]
mod tests {
    use super::sanitize_detail;

    #[test]
    fn removes_sensitive_details() {
        assert_eq!(sanitize_detail("Authorization: Bearer secret"), "redacted");
        assert_eq!(sanitize_detail("-----BEGIN PRIVATE KEY-----"), "redacted");
    }

    #[test]
    fn bounds_non_sensitive_details() {
        assert_eq!(sanitize_detail(" timeout "), "timeout");
        assert_eq!(sanitize_detail(&"x".repeat(161)).chars().count(), 160);
    }
}
