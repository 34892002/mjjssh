use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tree_sitter::{Node, Parser};

use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use super::models::{AiExecutableDecision, AiPendingAction, ExecutionMode, ExecutionScope};
use super::risk_confirmation::{RiskConfirmation, RiskConfirmationStore};

const MAX_TASKS_PER_SESSION: usize = 1;
const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const MAX_TERMINAL_CONTEXT_BYTES: usize = 8 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum AiTaskError {
    #[error("An AI task is already active for this SSH session")]
    TaskAlreadyActive,
    #[error("AI task not found: {0}")]
    TaskNotFound(String),
    #[error("AI task input is invalid: {0}")]
    InvalidInput(String),
    #[error("AI action not found or is no longer awaiting a decision")]
    ActionNotPending,
    #[error("AI action belongs to a different request")]
    ActionMismatch,
}

#[derive(Clone, Default)]
pub struct AiTaskManager {
    active_tasks: Arc<Mutex<HashMap<String, ActiveTask>>>,
}

/// The sole command preflight result for AI SSH execution.
///
/// It deliberately does not decide confirmation-mode grants or write audit logs:
/// those remain the responsibility of the grant store and the audit logger.
pub enum ExecutionDecision {
    Ready {
        command: String,
        executables: Vec<String>,
    },
    RequireRiskConfirmation {
        command: String,
        confirmation: RiskConfirmation,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SshSafetyContext {
    pub ssh_port: u16,
    pub client_ip: Option<String>,
    pub server_ip: Option<String>,
    pub route_interface: Option<String>,
    pub mounted_devices: HashSet<String>,
}

impl SshSafetyContext {
    pub fn from_probe(ssh_port: u16, output: &str) -> Self {
        const CONNECTION: &str = "__MYSSH_SAFETY_CONNECTION__";
        const ROUTE: &str = "__MYSSH_SAFETY_ROUTE__";
        const MOUNT: &str = "__MYSSH_SAFETY_MOUNT__";
        const BLOCK: &str = "__MYSSH_SAFETY_BLOCK__";

        let mut context = Self {
            ssh_port,
            ..Self::default()
        };
        let mut parent_by_device = HashMap::new();
        for line in output.lines() {
            if let Some(value) = line.strip_prefix(CONNECTION) {
                let parts = value.split_whitespace().collect::<Vec<_>>();
                context.client_ip = parts.first().map(|value| (*value).to_owned());
                context.server_ip = parts.get(2).map(|value| (*value).to_owned());
                if let Some(port) = parts.get(3).and_then(|value| value.parse().ok()) {
                    context.ssh_port = port;
                }
            } else if let Some(value) = line.strip_prefix(ROUTE) {
                let parts = value.split_whitespace().collect::<Vec<_>>();
                context.route_interface = parts
                    .windows(2)
                    .find(|parts| parts[0] == "dev")
                    .map(|parts| parts[1].to_owned());
            } else if let Some(value) = line.strip_prefix(MOUNT) {
                let device = value.trim();
                if device.starts_with("/dev/") {
                    context.mounted_devices.insert(device.to_owned());
                }
            } else if let Some(value) = line.strip_prefix(BLOCK) {
                let parts = value.split_whitespace().collect::<Vec<_>>();
                let Some(name) = parts.first() else {
                    continue;
                };
                let device = format!("/dev/{name}");
                if let Some(parent) = parts.get(1).filter(|parent| **parent != "-") {
                    parent_by_device.insert(device.clone(), format!("/dev/{parent}"));
                }
            }
        }
        let mounted_devices = context.mounted_devices.iter().cloned().collect::<Vec<_>>();
        for device in mounted_devices {
            let mut current = parent_by_device.get(&device).cloned();
            while let Some(device) = current {
                if !context.mounted_devices.insert(device.clone()) {
                    break;
                }
                current = parent_by_device.get(&device).cloned();
            }
        }
        context
    }
}

pub struct ExecutionGuard<'a> {
    risk_confirmations: &'a RiskConfirmationStore,
}

impl<'a> ExecutionGuard<'a> {
    pub fn new(risk_confirmations: &'a RiskConfirmationStore) -> Self {
        Self { risk_confirmations }
    }

    pub async fn evaluate(
        &self,
        mode: &ExecutionMode,
        session_id: &str,
        proposed_command: &str,
        safety_context: Option<&SshSafetyContext>,
        requested_risk_reason: Option<&str>,
    ) -> Result<ExecutionDecision, AiTaskError> {
        let command = normalize_proposed_command(proposed_command).to_owned();
        let executables = parse_pipeline_executables(&command)?;

        if !matches!(mode, ExecutionMode::Autonomous) {
            return Ok(ExecutionDecision::Ready {
                command,
                executables,
            });
        }

        let reason = ssh_connectivity_risk(&command, safety_context)
            .map(str::to_owned)
            .or_else(|| {
                requested_risk_reason
                    .map(str::trim)
                    .filter(|reason| !reason.is_empty())
                    .map(|reason| reason.chars().take(240).collect())
            });
        let Some(reason) = reason else {
            return Ok(ExecutionDecision::Ready {
                command,
                executables,
            });
        };

        let confirmation = self
            .risk_confirmations
            .create(session_id.to_owned(), command.clone(), reason)
            .await;
        Ok(ExecutionDecision::RequireRiskConfirmation {
            command,
            confirmation,
        })
    }
}

struct ActiveTask {
    session_id: String,
    cancellation_token: CancellationToken,
    pending_action: Option<PendingAction>,
}

struct PendingAction {
    action: AiPendingAction,
    decision: oneshot::Sender<Vec<AiExecutableDecision>>,
}

impl AiTaskManager {
    pub async fn start_task(
        &self,
        request_id: String,
        session_id: String,
    ) -> Result<CancellationToken, AiTaskError> {
        let mut tasks = self.active_tasks.lock().await;
        if tasks
            .values()
            .filter(|task| task.session_id == session_id)
            .count()
            >= MAX_TASKS_PER_SESSION
        {
            return Err(AiTaskError::TaskAlreadyActive);
        }

        let cancellation_token = CancellationToken::new();
        tasks.insert(
            request_id,
            ActiveTask {
                session_id,
                cancellation_token: cancellation_token.clone(),
                pending_action: None,
            },
        );
        Ok(cancellation_token)
    }

    pub async fn cancel_task(&self, request_id: &str) -> Result<(), AiTaskError> {
        let mut tasks = self.active_tasks.lock().await;
        let task = tasks
            .get_mut(request_id)
            .ok_or_else(|| AiTaskError::TaskNotFound(request_id.to_owned()))?;
        task.cancellation_token.cancel();
        task.pending_action.take();
        Ok(())
    }

    pub async fn cancel_tasks_for_session(&self, session_id: &str) {
        let mut tasks = self.active_tasks.lock().await;
        for task in tasks
            .values_mut()
            .filter(|task| task.session_id == session_id)
        {
            task.cancellation_token.cancel();
            task.pending_action.take();
        }
    }

    pub async fn wait_for_action_decision(
        &self,
        request_id: &str,
        action: AiPendingAction,
    ) -> Result<Option<Vec<AiExecutableDecision>>, AiTaskError> {
        let (sender, receiver) = oneshot::channel();
        let cancellation_token = {
            let mut tasks = self.active_tasks.lock().await;
            let task = tasks
                .get_mut(request_id)
                .ok_or_else(|| AiTaskError::TaskNotFound(request_id.to_owned()))?;
            if task.cancellation_token.is_cancelled() || task.pending_action.is_some() {
                return Err(AiTaskError::ActionNotPending);
            }
            task.pending_action = Some(PendingAction {
                action,
                decision: sender,
            });
            task.cancellation_token.clone()
        };

        tokio::select! {
            _ = cancellation_token.cancelled() => Ok(None),
            decision = receiver => Ok(decision.ok()),
        }
    }

    pub async fn decide_action(
        &self,
        request_id: &str,
        action_id: &str,
        decisions: Vec<AiExecutableDecision>,
    ) -> Result<(), AiTaskError> {
        let decision = {
            let mut tasks = self.active_tasks.lock().await;
            let task = tasks
                .get_mut(request_id)
                .ok_or_else(|| AiTaskError::TaskNotFound(request_id.to_owned()))?;
            if task.cancellation_token.is_cancelled() {
                return Err(AiTaskError::ActionNotPending);
            }
            let pending = task
                .pending_action
                .take()
                .ok_or(AiTaskError::ActionNotPending)?;
            if pending.action.action_id != action_id {
                task.pending_action = Some(pending);
                return Err(AiTaskError::ActionMismatch);
            }
            if let Err(error) = validate_decisions(&pending.action.missing_executables, &decisions)
            {
                task.pending_action = Some(pending);
                return Err(error);
            }
            pending.decision
        };
        let _ = decision.send(decisions);
        Ok(())
    }

    pub async fn session_for_task(&self, request_id: &str) -> Result<String, AiTaskError> {
        self.active_tasks
            .lock()
            .await
            .get(request_id)
            .map(|task| task.session_id.clone())
            .ok_or_else(|| AiTaskError::TaskNotFound(request_id.to_owned()))
    }

    pub async fn is_active(&self, request_id: &str) -> bool {
        self.active_tasks
            .lock()
            .await
            .get(request_id)
            .is_some_and(|task| !task.cancellation_token.is_cancelled())
    }

    pub async fn finish_task(&self, request_id: &str) {
        self.active_tasks.lock().await.remove(request_id);
    }
}

const SYSTEM_WHITELIST: &[&str] = &[
    "cat", "grep", "cut", "sort", "uniq", "head", "tail", "tr", "wc", "printf",
];

const MAX_COMMAND_BYTES: usize = 16 * 1024;
const INDIRECT_EXECUTION_COMMANDS: &[&str] = &[
    "eval", "source", ".", "sh", "bash", "dash", "zsh", "fish", "env", "busybox", "xargs",
];
const PRIVILEGE_WRAPPERS: &[&str] = &["sudo", "doas"];

/// Parses a Bash command block and returns every statically identifiable executable.
///
/// Authorization depends on the returned executable names. Commands that delegate
/// execution from dynamically constructed text are rejected because their actual
/// program list cannot be established before the user authorizes the action.
pub fn parse_pipeline_executables(command: &str) -> Result<Vec<String>, AiTaskError> {
    let command = normalize_proposed_command(command);
    let tree = parse_valid_bash_command(command)?;
    let mut executables = Vec::new();
    collect_executables(tree.root_node(), command.as_bytes(), &mut executables)?;
    if executables.is_empty() {
        return Err(AiTaskError::InvalidInput(
            "command block does not invoke an executable".into(),
        ));
    }
    Ok(executables)
}

pub fn validate_bash_syntax(command: &str) -> Result<(), AiTaskError> {
    let command = normalize_proposed_command(command);
    parse_valid_bash_command(command).map(|_| ())
}

/// Best-effort anti-footgun recognizer for only obvious SSH availability hazards.
///
/// This intentionally does not infer arbitrary program effects. Less obvious risks
/// are handled by the autonomous system prompt and the model's explicit request.
pub fn ssh_connectivity_risk(
    command: &str,
    context: Option<&SshSafetyContext>,
) -> Option<&'static str> {
    let command = command.to_ascii_lowercase();
    let has_any = |markers: &[&str]| markers.iter().any(|marker| command.contains(marker));

    if has_any(&["reboot", "shutdown", "poweroff", "halt"]) {
        return Some("该操作会重启或关闭服务器；重启后的 SSH 可达性无法由当前会话保证。");
    }
    if has_any(&[
        "systemctl restart ssh",
        "systemctl restart sshd",
        "systemctl reload ssh",
        "systemctl reload sshd",
        "systemctl stop ssh",
        "systemctl stop sshd",
        "systemctl disable ssh",
        "systemctl disable sshd",
        "systemctl mask ssh",
        "systemctl mask sshd",
        "service ssh restart",
        "service ssh stop",
    ]) {
        return Some("该操作会重启、停止或禁用 SSH 服务，可能中断当前连接或导致后续无法登录。");
    }
    let context = context?;
    let ssh_port = context.ssh_port.to_string();
    let touches_ssh_port = has_any(&[
        &format!("--dport {ssh_port}"),
        &format!("--dports {ssh_port}"),
        &format!("port {ssh_port}"),
        &format!(" {ssh_port}"),
        &format!(":{ssh_port}"),
    ]);
    let touches_client = context
        .client_ip
        .as_deref()
        .is_some_and(|client_ip| command.contains(client_ip));
    let touches_route_interface = context
        .route_interface
        .as_deref()
        .is_some_and(|interface| command.contains(interface));
    if (has_any(&["ufw deny", "ufw reject"]) && touches_ssh_port)
        || command.contains("firewall-cmd --remove-service=ssh")
        || (has_any(&["iptables", "nft "])
            && has_any(&[" -a ", " -i ", " -d ", "add rule", "delete rule", "flush"])
            && (touches_ssh_port || touches_client))
    {
        return Some(
            "该操作明确修改当前 SSH 连接所用端口或客户端来源的防火墙规则，可能立即失去连接。",
        );
    }
    if has_any(&["systemctl restart networking", "systemctl restart network"])
        || (has_any(&["nmcli con down", "nmcli connection down", "ip link set"])
            && touches_route_interface)
        || (has_any(&["ip addr del", "ip route del"])
            && (touches_route_interface
                || context
                    .server_ip
                    .as_deref()
                    .is_some_and(|server_ip| command.contains(server_ip))))
    {
        return Some("该操作明确影响当前 SSH 回程所用网络路径，可能立即断开会话。");
    }
    if has_any(&["mkfs", "wipefs"])
        && context
            .mounted_devices
            .iter()
            .any(|device| command.contains(device))
    {
        return Some("该操作会格式化或擦除当前已挂载文件系统所在设备，可能破坏正在运行的系统。");
    }
    if has_any(&["authorized_keys", "sshd_config"])
        && has_any(&[
            "> /root/.ssh",
            ">/root/.ssh",
            "> /etc/ssh",
            ">/etc/ssh",
            "tee /root/.ssh",
            "tee /etc/ssh",
            "sed -i",
            "perl -i",
        ])
    {
        return Some("该操作明确修改 SSH 认证或服务配置，可能导致后续无法通过 SSH 登录。");
    }
    None
}

fn parse_valid_bash_command(command: &str) -> Result<tree_sitter::Tree, AiTaskError> {
    if command.is_empty() || command.len() > MAX_COMMAND_BYTES {
        return Err(AiTaskError::InvalidInput(format!(
            "command must be between 1 and {} characters",
            MAX_COMMAND_BYTES
        )));
    }

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .map_err(|error| {
            AiTaskError::InvalidInput(format!("could not initialize Bash parser: {error}"))
        })?;
    let tree = parser
        .parse(command, None)
        .ok_or_else(|| AiTaskError::InvalidInput("could not parse command".into()))?;
    if tree.root_node().has_error() {
        return Err(AiTaskError::InvalidInput(
            "command contains incomplete or invalid Bash syntax".into(),
        ));
    }
    Ok(tree)
}

fn collect_executables(
    node: Node<'_>,
    source: &[u8],
    executables: &mut Vec<String>,
) -> Result<(), AiTaskError> {
    if node.kind() == "command" {
        collect_command_executables(node, source, executables)?;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_executables(child, source, executables)?;
    }
    Ok(())
}

fn collect_command_executables(
    node: Node<'_>,
    source: &[u8],
    executables: &mut Vec<String>,
) -> Result<(), AiTaskError> {
    let Some(name) = node.child_by_field_name("name") else {
        return Ok(());
    };
    let executable = source_text(name, source)?;
    if !is_bare_executable_name(executable) {
        return Err(AiTaskError::InvalidInput(
            "command names must be bare executable names so they can be authorized".into(),
        ));
    }
    if INDIRECT_EXECUTION_COMMANDS.contains(&executable) {
        return Err(AiTaskError::InvalidInput(format!(
            "{executable} is not supported because it can execute dynamically constructed commands"
        )));
    }
    add_executable(executable, executables);

    if executable == "find" {
        collect_find_exec_executables(node, source, executables)?;
    }
    if PRIVILEGE_WRAPPERS.contains(&executable) {
        collect_wrapper_target(node, source, executables)?;
    }
    Ok(())
}

fn collect_find_exec_executables(
    node: Node<'_>,
    source: &[u8],
    executables: &mut Vec<String>,
) -> Result<(), AiTaskError> {
    let arguments = command_argument_texts(node, source)?;
    let Some(exec_position) = arguments
        .iter()
        .position(|argument| matches!(*argument, "-exec" | "-execdir"))
    else {
        return Ok(());
    };
    let target = arguments
        .get(exec_position + 1)
        .ok_or_else(|| AiTaskError::InvalidInput("find -exec must include an executable".into()))?;
    if !is_bare_executable_name(target) || INDIRECT_EXECUTION_COMMANDS.contains(target) {
        return Err(AiTaskError::InvalidInput(
            "find -exec target must be a supported bare executable name".into(),
        ));
    }
    add_executable(target, executables);
    Ok(())
}

fn collect_wrapper_target(
    node: Node<'_>,
    source: &[u8],
    executables: &mut Vec<String>,
) -> Result<(), AiTaskError> {
    let arguments = command_argument_texts(node, source)?;
    let target = arguments
        .iter()
        .find(|argument| !argument.starts_with('-'))
        .ok_or_else(|| {
            AiTaskError::InvalidInput("privilege wrapper must include an executable".into())
        })?;
    if !is_bare_executable_name(target) || INDIRECT_EXECUTION_COMMANDS.contains(target) {
        return Err(AiTaskError::InvalidInput(
            "privilege wrapper target must be a supported bare executable name".into(),
        ));
    }
    add_executable(target, executables);
    Ok(())
}

fn command_argument_texts<'a>(
    node: Node<'_>,
    source: &'a [u8],
) -> Result<Vec<&'a str>, AiTaskError> {
    let mut cursor = node.walk();
    node.children_by_field_name("argument", &mut cursor)
        .map(|argument| source_text(argument, source))
        .collect()
}

fn source_text<'a>(node: Node<'_>, source: &'a [u8]) -> Result<&'a str, AiTaskError> {
    std::str::from_utf8(&source[node.byte_range()])
        .map_err(|_| AiTaskError::InvalidInput("command must be valid UTF-8".into()))
}

fn is_bare_executable_name(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

fn add_executable(executable: &str, executables: &mut Vec<String>) {
    if !executables.iter().any(|existing| existing == executable) {
        executables.push(executable.to_owned());
    }
}

pub fn normalize_proposed_command(command: &str) -> &str {
    let command = command.trim();
    let Some(without_opening_fence) = command.strip_prefix("```") else {
        return command;
    };
    let Some(first_newline) = without_opening_fence.find('\n') else {
        return command;
    };
    let body = &without_opening_fence[first_newline + 1..];
    body.strip_suffix("```").map(str::trim).unwrap_or(command)
}

pub fn is_system_whitelisted(executable: &str) -> bool {
    SYSTEM_WHITELIST.contains(&executable)
}

fn validate_decisions(
    expected: &[String],
    decisions: &[AiExecutableDecision],
) -> Result<(), AiTaskError> {
    if decisions.len() != expected.len() {
        return Err(AiTaskError::InvalidInput(
            "a decision is required for every executable".into(),
        ));
    }
    let mut names = std::collections::HashSet::new();
    for decision in decisions {
        if !expected
            .iter()
            .any(|expected| expected == &decision.executable)
            || !names.insert(&decision.executable)
        {
            return Err(AiTaskError::InvalidInput(
                "decisions do not match the pending executables".into(),
            ));
        }
    }
    Ok(())
}

pub fn validate_task_input(
    conversation_id: &str,
    messages: &[super::models::AiChatMessage],
    execution_mode: &ExecutionMode,
    scopes: &[ExecutionScope],
    terminal_context: Option<&str>,
    include_terminal_context: bool,
) -> Result<(), AiTaskError> {
    if conversation_id.trim().is_empty() {
        return Err(AiTaskError::InvalidInput(
            "conversation_id is required".into(),
        ));
    }
    let has_read_only_scope = scopes
        .iter()
        .any(|scope| matches!(scope, ExecutionScope::ReadOnlyDiagnostics));
    let has_command_scope = scopes
        .iter()
        .any(|scope| matches!(scope, ExecutionScope::CommandExecution));
    match execution_mode {
        ExecutionMode::ReadOnly if !has_read_only_scope || has_command_scope => {
            return Err(AiTaskError::InvalidInput(
                "analysis mode requires only the read_only_diagnostics scope".into(),
            ));
        }
        ExecutionMode::ApprovalRequired | ExecutionMode::Autonomous if !has_command_scope => {
            return Err(AiTaskError::InvalidInput(
                "execution modes require the command_execution scope".into(),
            ));
        }
        _ => {}
    }
    if messages.is_empty() {
        return Err(AiTaskError::InvalidInput(
            "at least one message is required".into(),
        ));
    }

    if messages
        .iter()
        .any(|message| message.content.trim().is_empty())
    {
        return Err(AiTaskError::InvalidInput("messages cannot be empty".into()));
    }

    if messages
        .iter()
        .any(|message| message.content.len() > MAX_MESSAGE_BYTES)
    {
        return Err(AiTaskError::InvalidInput(
            "a message exceeds the 8 KiB limit".into(),
        ));
    }

    if include_terminal_context {
        let context = terminal_context.ok_or_else(|| {
            AiTaskError::InvalidInput("terminal context is required when enabled".into())
        })?;
        if context.len() > MAX_TERMINAL_CONTEXT_BYTES {
            return Err(AiTaskError::InvalidInput(
                "terminal context exceeds the 8 KiB limit".into(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execution_guard_applies_shared_preflight_to_both_execution_modes() {
        let risk_confirmations = RiskConfirmationStore::default();
        let guard = ExecutionGuard::new(&risk_confirmations);

        let ExecutionDecision::Ready {
            command,
            executables,
        } = guard
            .evaluate(
                &ExecutionMode::ApprovalRequired,
                "session-a",
                "```sh\nfind / -type f -exec ls -la {} \\;\n```",
                None,
                None,
            )
            .await
            .unwrap()
        else {
            panic!("approval mode must not request risk confirmation");
        };
        assert_eq!(command, "find / -type f -exec ls -la {} \\;");
        assert_eq!(executables, vec!["find", "ls"]);

        assert!(guard
            .evaluate(
                &ExecutionMode::Autonomous,
                "session-a",
                "eval 'systemctl restart ssh'",
                None,
                None,
            )
            .await
            .is_err());

        let ExecutionDecision::RequireRiskConfirmation {
            command,
            confirmation,
        } = guard
            .evaluate(
                &ExecutionMode::Autonomous,
                "session-a",
                "systemctl restart ssh",
                Some(&SshSafetyContext::default()),
                None,
            )
            .await
            .unwrap()
        else {
            panic!("autonomous SSH restart must require risk confirmation");
        };
        assert_eq!(confirmation.command, command);
        let ExecutionDecision::RequireRiskConfirmation { .. } = guard
            .evaluate(
                &ExecutionMode::Autonomous,
                "session-a",
                "printf '%s' safe",
                None,
                Some("AI 判断该命令可能影响 SSH 连通性"),
            )
            .await
            .unwrap()
        else {
            panic!("AI-requested confirmation must require user approval");
        };
    }

    #[test]
    fn incomplete_bash_syntax_is_rejected_before_terminal_execution() {
        assert!(validate_bash_syntax("echo 'unterminated").is_err());
        assert!(validate_bash_syntax("printf '%s' complete").is_ok());
    }

    #[test]
    fn administrator_commands_are_not_rejected_for_resource_cost_or_file_scope() {
        let command = "find / -type f -mtime -7 -exec ls -la {} \\; 2>/dev/null | grep -E '\\.(sh|py|pl|rb|php|cgi|conf|cfg)$' | head -30";
        assert!(validate_bash_syntax(command).is_ok());
        assert!(parse_pipeline_executables(command).is_ok());
    }

    #[test]
    fn detects_only_current_session_connectivity_and_mounted_disk_risks() {
        let context = SshSafetyContext {
            ssh_port: 2222,
            client_ip: Some("198.51.100.10".into()),
            server_ip: Some("192.0.2.10".into()),
            route_interface: Some("ens3".into()),
            mounted_devices: ["/dev/sda", "/dev/sda1"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        };
        assert!(ssh_connectivity_risk("cat /root/.ssh/authorized_keys", Some(&context)).is_none());
        assert!(ssh_connectivity_risk(
            "cat /root/.ssh/authorized_keys 2>/dev/null",
            Some(&context)
        )
        .is_none());
        assert!(
            ssh_connectivity_risk("echo key > /root/.ssh/authorized_keys", Some(&context))
                .is_some()
        );
        assert!(ssh_connectivity_risk("systemctl restart sshd", Some(&context)).is_some());
        assert!(ssh_connectivity_risk("ufw deny 22", Some(&context)).is_none());
        assert!(ssh_connectivity_risk("ufw deny 2222", Some(&context)).is_some());
        assert!(ssh_connectivity_risk(
            "iptables -A INPUT -p tcp --dport 2222 -j DROP",
            Some(&context)
        )
        .is_some());
        assert!(ssh_connectivity_risk(
            "iptables -A INPUT -s 198.51.100.10 -j DROP",
            Some(&context)
        )
        .is_some());
        assert!(ssh_connectivity_risk(
            "iptables -A INPUT -p tcp --dport 443 -j ACCEPT",
            Some(&context)
        )
        .is_none());
        assert!(ssh_connectivity_risk("ip link set ens4 down", Some(&context)).is_none());
        assert!(ssh_connectivity_risk("ip link set ens3 down", Some(&context)).is_some());
        assert!(ssh_connectivity_risk("reboot", Some(&context)).is_some());
        assert!(ssh_connectivity_risk("mkfs.ext4 /dev/sda1", Some(&context)).is_some());
        assert!(ssh_connectivity_risk("wipefs -a /dev/sdb", Some(&context)).is_none());
        assert!(ssh_connectivity_risk("ufw status", Some(&context)).is_none());
        assert!(ssh_connectivity_risk("iptables -L", Some(&context)).is_none());
        assert!(ssh_connectivity_risk("ip route show", Some(&context)).is_none());
        assert!(
            ssh_connectivity_risk("find / -type f -exec ls -la {} \\;", Some(&context)).is_none()
        );
        assert!(ssh_connectivity_risk("apt install nginx", Some(&context)).is_none());
        assert!(ssh_connectivity_risk("rm /tmp/file", Some(&context)).is_none());
    }

    #[test]
    fn parses_connection_route_and_mounted_device_context() {
        let context = SshSafetyContext::from_probe(
            22,
            "__MYSSH_SAFETY_CONNECTION__198.51.100.10 52123 192.0.2.10 2222\n__MYSSH_SAFETY_ROUTE__192.0.2.10 via 10.0.0.1 dev ens3 src 10.0.0.2\n__MYSSH_SAFETY_MOUNT__/dev/sda1\n__MYSSH_SAFETY_BLOCK__sda - \n__MYSSH_SAFETY_BLOCK__sda1 sda /\n__MYSSH_SAFETY_BLOCK__sdb - \n",
        );
        assert_eq!(context.ssh_port, 2222);
        assert_eq!(context.client_ip.as_deref(), Some("198.51.100.10"));
        assert_eq!(context.server_ip.as_deref(), Some("192.0.2.10"));
        assert_eq!(context.route_interface.as_deref(), Some("ens3"));
        assert!(context.mounted_devices.contains("/dev/sda"));
        assert!(context.mounted_devices.contains("/dev/sda1"));
        assert!(!context.mounted_devices.contains("/dev/sdb"));
        assert!(ssh_connectivity_risk("reboot", None).is_some());
        assert!(ssh_connectivity_risk("ufw deny 2222", None).is_none());
        assert!(ssh_connectivity_risk("mkfs.ext4 /dev/sda1", None).is_none());
    }

    #[test]
    fn confirmation_mode_requires_command_scope() {
        let messages = [super::super::models::AiChatMessage {
            role: super::super::models::AiMessageRole::User,
            content: "check disk usage".into(),
        }];
        assert!(validate_task_input(
            "current",
            &messages,
            &ExecutionMode::ApprovalRequired,
            &[ExecutionScope::ReadOnlyDiagnostics],
            None,
            false,
        )
        .is_err());
        assert!(validate_task_input(
            "current",
            &messages,
            &ExecutionMode::ApprovalRequired,
            &[ExecutionScope::CommandExecution],
            None,
            false,
        )
        .is_ok());
    }

    #[test]
    fn autonomous_mode_requires_and_accepts_command_scope() {
        let messages = [super::super::models::AiChatMessage {
            role: super::super::models::AiMessageRole::User,
            content: "inspect and repair the service".into(),
        }];
        assert!(validate_task_input(
            "current",
            &messages,
            &ExecutionMode::Autonomous,
            &[ExecutionScope::ReadOnlyDiagnostics],
            None,
            false,
        )
        .is_err());
        assert!(validate_task_input(
            "current",
            &messages,
            &ExecutionMode::Autonomous,
            &[ExecutionScope::CommandExecution],
            None,
            false,
        )
        .is_ok());
    }

    #[test]
    fn parses_unique_executables_from_simple_command_expressions() {
        assert_eq!(
            parse_pipeline_executables("lscpu | grep -E 'Architecture|CPU\\(s\\)' | grep CPU")
                .unwrap(),
            vec!["lscpu", "grep"],
        );
        assert_eq!(
            parse_pipeline_executables("lscpu && free -h && lsblk").unwrap(),
            vec!["lscpu", "free", "lsblk"],
        );
        assert_eq!(
            parse_pipeline_executables("lscpu || free -h").unwrap(),
            vec!["lscpu", "free"],
        );
        assert_eq!(
            parse_pipeline_executables("df -h; rm -rf /").unwrap(),
            vec!["df", "rm"],
        );
        assert_eq!(
            parse_pipeline_executables("```sh\nlscpu | grep CPU\n``` ").unwrap(),
            vec!["lscpu", "grep"],
        );
        assert_eq!(
            parse_pipeline_executables("curl https://example.com > output").unwrap(),
            vec!["curl"],
        );
        assert_eq!(
            parse_pipeline_executables("echo \"$(uname)\"").unwrap(),
            vec!["echo", "uname"],
        );
        assert!(parse_pipeline_executables("sh -c 'uname'").is_err());
    }

    #[test]
    fn system_whitelist_is_limited_to_filter_utilities() {
        assert!(is_system_whitelisted("cat"));
        assert!(is_system_whitelisted("grep"));
        assert!(is_system_whitelisted("sort"));
        assert!(!is_system_whitelisted("lscpu"));
        assert!(!is_system_whitelisted("sed"));
    }

    #[test]
    fn decisions_must_exactly_match_missing_executables() {
        let expected = vec!["lscpu".into(), "rm".into()];
        let decisions = vec![
            AiExecutableDecision {
                executable: "lscpu".into(),
                grant: super::super::models::AiExecutableGrant::Once,
            },
            AiExecutableDecision {
                executable: "rm".into(),
                grant: super::super::models::AiExecutableGrant::Server,
            },
        ];
        assert!(validate_decisions(&expected, &decisions).is_ok());
        assert!(validate_decisions(&expected, &decisions[..1]).is_err());
    }

    #[test]
    fn requires_a_conversation_and_read_only_scope() {
        let messages = [super::super::models::AiChatMessage {
            role: super::super::models::AiMessageRole::User,
            content: "check disk usage".into(),
        }];
        assert!(validate_task_input(
            "",
            &messages,
            &ExecutionMode::ReadOnly,
            &[ExecutionScope::ReadOnlyDiagnostics],
            None,
            false,
        )
        .is_err());
        assert!(validate_task_input(
            "current",
            &messages,
            &ExecutionMode::ReadOnly,
            &[],
            None,
            false,
        )
        .is_err());
    }
}
