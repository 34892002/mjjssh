use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::state::AppState;

const DIAGNOSTICS_LOG_FILES: [&str; 3] =
    ["diagnostics.log", "diagnostics.log.1", "diagnostics.log.2"];

pub async fn export_archive(state: &AppState) -> Result<PathBuf, String> {
    let downloads_dir = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|home| home.join("Downloads"))
        .unwrap_or_else(|| state.app_dir.clone());
    fs::create_dir_all(&downloads_dir).map_err(|error| error.to_string())?;
    let destination = downloads_dir.join(format!(
        "mjjssh-diagnostics-{}.zip",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));

    let file = File::create(&destination).map_err(|error| error.to_string())?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let profile_count = state
        .with_vault(|vault| Ok(vault.list_profiles()?.len()))
        .await
        .unwrap_or(0);
    let key_count = state
        .with_vault(|vault| Ok(vault.list_keys()?.len()))
        .await
        .unwrap_or(0);
    let system_info = format!(
        "MJJSSH diagnostic export\ncreated_at={}\napp_version={}\nos={}\narchitecture={}\n\nIncluded data is limited to safe diagnostics logs, local crash reports, and configuration counts.\nOriginal app.log, ai.log, vault data, credentials, terminal content, and remote file content are not included.\n",
        chrono::Utc::now().to_rfc3339(),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    write_text(&mut archive, "system-info.txt", &system_info, options)?;
    let configuration_summary =
        format!("profiles_count={profile_count}\nssh_keys_count={key_count}\n",);
    write_text(
        &mut archive,
        "configuration-summary.txt",
        &configuration_summary,
        options,
    )?;

    let log_dir = state.app_dir.join("logs");
    for name in DIAGNOSTICS_LOG_FILES {
        add_file_if_exists(
            &mut archive,
            &log_dir.join(name),
            &format!("safe-logs/{name}"),
            options,
        )?;
    }
    let reports_dir = state.app_dir.join("crash-reports");
    if let Ok(entries) = fs::read_dir(reports_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("txt") {
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    add_file_if_exists(
                        &mut archive,
                        &path,
                        &format!("crash-reports/{name}"),
                        options,
                    )?;
                }
            }
        }
    }

    archive.finish().map_err(|error| error.to_string())?;
    crate::diagnostics::log::event("diagnostics", "exported", "success");
    Ok(destination)
}

fn add_file_if_exists(
    archive: &mut ZipWriter<File>,
    source: &Path,
    destination: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    let Ok(mut source_file) = File::open(source) else {
        return Ok(());
    };
    archive
        .start_file(destination, options)
        .map_err(|error| error.to_string())?;
    io::copy(&mut source_file, archive).map_err(|error| error.to_string())?;
    Ok(())
}

fn write_text(
    archive: &mut ZipWriter<File>,
    name: &str,
    contents: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    archive
        .start_file(name, options)
        .map_err(|error| error.to_string())?;
    archive
        .write_all(contents.as_bytes())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::DIAGNOSTICS_LOG_FILES;

    #[test]
    fn exports_only_the_safe_log_file_family() {
        assert_eq!(
            DIAGNOSTICS_LOG_FILES,
            ["diagnostics.log", "diagnostics.log.1", "diagnostics.log.2"]
        );
        assert!(!DIAGNOSTICS_LOG_FILES.contains(&"app.log"));
        assert!(!DIAGNOSTICS_LOG_FILES.contains(&"ai.log"));
        assert!(!DIAGNOSTICS_LOG_FILES.contains(&"vault.json"));
    }
}
