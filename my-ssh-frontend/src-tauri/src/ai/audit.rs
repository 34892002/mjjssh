use std::time::Duration;

use super::models::{AiPendingAction, ExecutionMode};

const MAX_ERROR_CHARS: usize = 1_024;

/// Writes sanitized AI lifecycle and action summaries to the independent AI log.
///
/// This module intentionally has no authority to permit, block, or execute commands.
pub fn task_started(task_id: &str, session_id: &str, mode: &ExecutionMode, message_count: usize) {
    crate::ai::log::info(format!(
        "task_id={task_id} event=started session_id={session_id} mode={mode:?} message_count={message_count}"
    ));
}

pub fn retry_scheduled(task_id: &str, attempt: usize) {
    crate::ai::log::warn(format!(
        "task_id={task_id} event=retry_scheduled attempt={attempt}/3"
    ));
}

pub fn task_finished(task_id: &str, outcome: &str, elapsed: Duration, error: Option<&str>) {
    let error = error.map(redact_error).unwrap_or_default();
    crate::ai::log::info(format!(
        "task_id={task_id} event=finished outcome={outcome} elapsed_ms={} error={error:?}",
        elapsed.as_millis(),
    ));
}

pub fn cancellation_requested(task_id: &str) {
    crate::ai::log::info(format!(
        "task_id={task_id} event=cancel_requested source=user"
    ));
}

pub fn action(
    task_id: &str,
    action: &AiPendingAction,
    session_id: &str,
    scope: &str,
    status: &str,
    decision: Option<&str>,
    result: Option<&str>,
) {
    crate::ai::log::info(format!(
        "task_id={task_id} action_id={} session_id={session_id} scope={scope} status={status} risk_level={} decision={} command={:?} purpose={:?} result={:?}",
        action.action_id,
        action.risk_level,
        decision.unwrap_or("none"),
        action.command,
        action.purpose,
        result_summary(result),
    ));
}

pub fn provider_status_retry(status: reqwest::StatusCode, attempt: usize, max_retries: usize) {
    crate::ai::log::warn(format!(
        "provider returned {status}; retrying request ({attempt}/{max_retries})"
    ));
}

pub fn provider_request_retry(kind: &str, detail: &str, attempt: usize, max_retries: usize) {
    crate::ai::log::warn(format!(
        "provider request failed; phase=chat_completion kind={kind} detail={:?} retrying ({attempt}/{max_retries})",
        redact_error(detail),
    ));
}

pub fn provider_request_started(message_count: usize, payload_bytes: usize) {
    crate::ai::log::info(format!(
        "provider request started; phase=chat_completion message_count={message_count} payload_bytes={payload_bytes}"
    ));
}

pub fn provider_stream_retry(attempt: usize, max_retries: usize) {
    crate::ai::log::warn(format!(
        "provider stream request failed; retrying ({attempt}/{max_retries})"
    ));
}

fn result_summary(result: Option<&str>) -> &'static str {
    match result {
        Some(output) if !output.trim().is_empty() => {
            if output.contains("BEGIN ")
                || output.contains("PRIVATE KEY")
                || output.contains("authorized_keys")
                || output.contains("/etc/shadow")
                || output.contains("token")
                || output.contains("password")
                || output.contains("api_key")
            {
                "redacted_sensitive_or_unstructured_output"
            } else {
                "output_present_not_logged"
            }
        }
        _ => "no_output",
    }
}

fn redact_error(error: &str) -> String {
    let mut redacted = error
        .split_whitespace()
        .map(|part| {
            if part.starts_with("http://") || part.starts_with("https://") {
                part.split('?').next().unwrap_or(part)
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if redacted.chars().count() > MAX_ERROR_CHARS {
        redacted = format!(
            "{} [truncated]",
            redacted.chars().take(MAX_ERROR_CHARS).collect::<String>()
        );
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::{redact_error, result_summary, MAX_ERROR_CHARS};

    #[test]
    fn errors_exclude_url_query_parameters() {
        assert_eq!(
            redact_error("request failed for https://example.test/v1/chat?token=secret"),
            "request failed for https://example.test/v1/chat"
        );
    }

    #[test]
    fn action_results_never_include_terminal_output() {
        assert_eq!(result_summary(None), "no_output");
        assert_eq!(
            result_summary(Some("service restarted")),
            "output_present_not_logged"
        );
        assert_eq!(
            result_summary(Some("-----BEGIN PRIVATE KEY-----")),
            "redacted_sensitive_or_unstructured_output"
        );
    }

    #[test]
    fn errors_are_bounded() {
        let error = "x".repeat(MAX_ERROR_CHARS + 1);
        assert!(redact_error(&error).ends_with(" [truncated]"));
    }
}
