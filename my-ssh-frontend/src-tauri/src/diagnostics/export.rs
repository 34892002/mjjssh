use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::state::AppState;

const APP_LOG_FILES: [&str; 3] = ["app.log", "app.log.1", "app.log.2"];
const SENSITIVE_MARKERS: [&str; 14] = [
    "password",
    "token",
    "api_key",
    "api key",
    "authorization",
    "private key",
    "begin ",
    "bearer ",
    "ssh-rsa",
    "ssh-ed25519",
    "terminal input",
    "terminal output",
    "ssh-data",
    "command=",
];
const IDENTIFYING_MARKERS: [&str; 8] = [
    "host=",
    "hostname=",
    "username=",
    "user=",
    "address=",
    "remote_path=",
    "local_path=",
    "session_id=",
];

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
        "MJJSSH diagnostic export\ncreated_at={}\napp_version={}\nos={}\narchitecture={}\n\nIncluded data is limited to a sanitized copy of application logs, local crash reports, and configuration counts.\nRaw app.log, ai.log, vault data, credentials, terminal content, and remote file content are not included.\n",
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
    for name in APP_LOG_FILES {
        add_sanitized_log_if_exists(
            &mut archive,
            &log_dir.join(name),
            &format!("safe-logs/{name}"),
            options,
        )?;
    }
    let reports_dir = state.app_dir.join("logs");
    if let Ok(entries) = fs::read_dir(reports_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("txt") {
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    add_file_if_exists(&mut archive, &path, &format!("safe-logs/{name}"), options)?;
                }
            }
        }
    }

    archive.finish().map_err(|error| error.to_string())?;
    log::info!("Diagnostic bundle exported");
    Ok(destination)
}

fn add_file_if_exists(
    archive: &mut ZipWriter<File>,
    source: &Path,
    destination: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    let Ok(contents) = fs::read_to_string(source) else {
        return Ok(());
    };
    write_text(archive, destination, &contents, options)
}

fn add_sanitized_log_if_exists(
    archive: &mut ZipWriter<File>,
    source: &Path,
    destination: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    let Ok(contents) = fs::read_to_string(source) else {
        return Ok(());
    };
    write_text(
        archive,
        destination,
        &sanitize_runtime_log(&contents),
        options,
    )
}

fn sanitize_runtime_log(contents: &str) -> String {
    contents
        .lines()
        .map(sanitize_runtime_log_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_runtime_log_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if SENSITIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return "[redacted sensitive log entry]".into();
    }

    let mut sanitized = line.to_owned();
    for marker in IDENTIFYING_MARKERS {
        if let Some(index) = lower.find(marker) {
            let end = sanitized[index..]
                .find(char::is_whitespace)
                .map(|offset| index + offset)
                .unwrap_or(sanitized.len());
            sanitized.replace_range(index..end, "[redacted identifier]");
        }
    }
    redact_url_or_path_tokens(&sanitized)
}

fn redact_url_or_path_tokens(line: &str) -> String {
    line.split_whitespace()
        .map(|token| {
            let trimmed =
                token.trim_matches(|character: char| matches!(character, ',' | ';' | ')' | '('));
            if trimmed.starts_with("http://")
                || trimmed.starts_with("https://")
                || trimmed.starts_with('/')
                || trimmed.starts_with("\\\\")
                || (trimmed.len() > 2
                    && trimmed.as_bytes()[1] == b':'
                    && matches!(trimmed.as_bytes()[0], b'A'..=b'Z' | b'a'..=b'z'))
            {
                "[redacted location]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    use super::{sanitize_runtime_log, APP_LOG_FILES};

    #[test]
    fn exports_the_application_log_family_without_ai_logs() {
        assert_eq!(APP_LOG_FILES, ["app.log", "app.log.1", "app.log.2"]);
        assert!(!APP_LOG_FILES.contains(&"ai.log"));
        assert!(!APP_LOG_FILES.contains(&"vault.json"));
    }

    #[test]
    fn removes_sensitive_and_identifying_log_entries() {
        let logs = "INFO host=server.example password=secret\nWARN remote_path=/var/lib/app\nINFO connected";
        let sanitized = sanitize_runtime_log(logs);

        assert!(sanitized.contains("[redacted sensitive log entry]"));
        assert!(sanitized.contains("[redacted identifier]"));
        assert!(!sanitized.contains("server.example"));
        assert!(!sanitized.contains("secret"));
        assert!(!sanitized.contains("/var/lib/app"));
    }
}
