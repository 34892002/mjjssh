use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const MAX_MESSAGE_CHARS: usize = 1_024;

pub fn install_panic_hook(crash_reports_dir: &Path) {
    let crash_reports_dir = crash_reports_dir.to_path_buf();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown".into());
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(String::as_str)
            })
            .unwrap_or("non-string panic payload");
        write_report(
            &crash_reports_dir,
            "rust-panic",
            &location,
            payload,
            Backtrace::force_capture().to_string(),
        );
    }));
}

pub fn write_frontend_report(
    crash_reports_dir: &Path,
    kind: &str,
    message: &str,
    stack: Option<&str>,
) {
    write_report(
        crash_reports_dir,
        kind,
        "frontend",
        message,
        stack.unwrap_or("not available").to_owned(),
    );
}

fn write_report(crash_reports_dir: &Path, kind: &str, source: &str, message: &str, stack: String) {
    if fs::create_dir_all(crash_reports_dir).is_err() {
        return;
    }
    let file_name = format!(
        "{}-{}.txt",
        chrono::Local::now().format("%Y%m%d-%H%M%S%.3f"),
        kind
    );
    let path = crash_reports_dir.join(file_name);
    let message = sanitize(message);
    let backtrace = sanitize(&stack);
    let content = format!(
        "MJJSSH crash report\ncreated_at={}\nkind={}\nsource={}\nmessage={:?}\n\nstack:\n{}\n",
        chrono::Local::now().to_rfc3339(),
        kind,
        source,
        message,
        backtrace
    );
    if let Ok(mut file) = OpenOptions::new().write(true).create_new(true).open(path) {
        let _ = file.write_all(content.as_bytes());
    }
}

fn sanitize(value: &str) -> String {
    let value = value
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if lower.contains("password")
                || lower.contains("token")
                || lower.contains("api_key")
                || lower.contains("authorization")
                || lower.contains("private") && lower.contains("key")
                || lower.starts_with("bearer")
            {
                "[redacted]"
            } else if part.starts_with("http://") || part.starts_with("https://") {
                part.split('?').next().unwrap_or(part)
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if value.chars().count() > MAX_MESSAGE_CHARS {
        format!(
            "{} [truncated]",
            value.chars().take(MAX_MESSAGE_CHARS).collect::<String>()
        )
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn redact_sensitive_error_parts() {
        assert_eq!(
            sanitize("Bearer secret token=value"),
            "[redacted] secret [redacted]"
        );
    }

    #[test]
    fn redacts_urls_with_sensitive_query_parameters() {
        assert_eq!(
            sanitize("https://example.test/path?token=secret"),
            "[redacted]"
        );
    }
}
