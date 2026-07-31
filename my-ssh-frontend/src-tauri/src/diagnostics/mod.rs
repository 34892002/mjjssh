pub mod export;
pub mod report;

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub fn install_panic_hook(app_dir: impl Into<std::path::PathBuf>) {
    report::install_panic_hook(&app_dir.into().join("logs"));
}

pub fn record_frontend_crash(app_dir: &Path, kind: &str, message: &str, stack: Option<&str>) {
    report::write_frontend_report(&app_dir.join("logs"), kind, message, stack);
}

pub fn record_backend_startup_error(app_dir: &Path, error: &str) {
    report::write_backend_report(&app_dir.join("logs"), "tauri-startup", error);
}

pub fn record_startup_progress(app_dir: &Path, stage: &str) {
    let log_dir = app_dir.join("logs");
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let path = log_dir.join("startup.log");
    if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path) {
        let _ = writeln!(file, "{} {stage}", chrono::Local::now().to_rfc3339());
    }
}
