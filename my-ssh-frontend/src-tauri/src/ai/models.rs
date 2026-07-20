use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiConnectionTestStatus {
    Success,
    AuthenticationFailed,
    ModelUnavailable,
    RateLimited,
    ServiceUnavailable,
    Timeout,
    NetworkError,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConnectionTestResult {
    pub status: AiConnectionTestStatus,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    ReadOnly,
    ApprovalRequired,
    Autonomous,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionScope {
    ReadOnlyDiagnostics,
    CommandExecution,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiImageInput {
    pub data_url: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMessage {
    pub role: AiMessageRole,
    pub content: String,
    #[serde(default)]
    pub images: Vec<AiImageInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAiTaskInput {
    pub request_id: String,
    pub session_id: String,
    pub conversation_id: String,
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub messages: Vec<AiChatMessage>,
    pub execution_mode: ExecutionMode,
    pub scopes: Vec<ExecutionScope>,
    pub include_terminal_context: bool,
    pub terminal_context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskStarted {
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiStreamEventType {
    Delta,
    Plan,
    ActionPending,
    RiskConfirmationRequired,
    ActionStarted,
    ActionCompleted,
    TaskStatus,
    PolicyRejected,
    Completed,
    Cancelled,
    Error,
}

#[cfg(test)]
mod tests {
    use super::AiStreamEventType;

    #[test]
    fn task_status_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_value(AiStreamEventType::TaskStatus).unwrap(),
            serde_json::json!("task_status")
        );
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPendingAction {
    pub action_id: String,
    pub command: String,
    pub purpose: String,
    pub expected_impact: String,
    pub risk_level: String,
    pub rollback_hint: String,
    pub missing_executables: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_confirmation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiActionResult {
    pub action_id: String,
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmAiRiskActionInput {
    pub session_id: String,
    pub action_id: String,
    pub risk_confirmation_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStreamEvent {
    pub request_id: String,
    pub event_type: AiStreamEventType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<AiPendingAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_result: Option<AiActionResult>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiExecutableGrant {
    Once,
    Server,
    Global,
    Reject,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExecutableDecision {
    pub executable: String,
    pub grant: AiExecutableGrant,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiActionDecisionInput {
    pub request_id: String,
    pub action_id: String,
    pub decisions: Vec<AiExecutableDecision>,
}
