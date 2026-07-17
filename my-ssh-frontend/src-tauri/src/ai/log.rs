use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
const ROTATED_LOGS: usize = 2;

static AI_LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn initialize(path: PathBuf) {
    let _ = AI_LOG_PATH.set(path);
}

pub fn info(message: impl AsRef<str>) {
    write("INFO", message.as_ref());
}

pub fn warn(message: impl AsRef<str>) {
    write("WARN", message.as_ref());
}

fn write(level: &str, message: &str) {
    let Some(path) = AI_LOG_PATH.get() else {
        return;
    };
    let Ok(_lock) = WRITE_LOCK.lock() else {
        return;
    };
    if rotate_if_needed(path).is_err() {
        return;
    }
    let line = format!(
        "{} [{level}] {message}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
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
    fs::rename(path, rotated_path(path, 1))?;
    Ok(())
}

fn rotated_path(path: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_names_follow_active_log_name() {
        assert_eq!(
            rotated_path(Path::new("data/logs/ai.log"), 2),
            PathBuf::from("data/logs/ai.log.2")
        );
    }
}
