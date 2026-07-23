pub mod export;
pub mod log;
pub mod report;

use std::path::Path;

pub fn initialize(app_dir: &Path) {
    log::initialize(app_dir.join("logs").join("diagnostics.log"));
}

pub fn install_panic_hook(app_dir: impl Into<std::path::PathBuf>) {
    report::install_panic_hook(&app_dir.into().join("crash-reports"));
}

pub fn record_frontend_crash(app_dir: &Path, kind: &str, message: &str, stack: Option<&str>) {
    report::write_frontend_report(&app_dir.join("crash-reports"), kind, message, stack);
    log::error("frontend", kind);
}
