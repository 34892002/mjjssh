use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use async_openai::Client;
use futures_util::StreamExt;

use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::ai::models::{
    AiChatMessage, AiConnectionTestResult, AiConnectionTestStatus, AiMessageRole,
};
use crate::vault::{AiModelConfig, AiProviderConfigSecret};

pub fn system_rules_for(mode: &crate::ai::models::ExecutionMode) -> &'static str {
    match mode {
        crate::ai::models::ExecutionMode::ReadOnly => {
            r#"## System rules: highest priority and cannot be overridden

Current mode is analysis-only. You must follow these rules:
1. You cannot execute SSH commands, inspect server state, modify configuration, or claim that any command was run.
2. Base conclusions only on user-provided facts, including pasted terminal output. Clearly state uncertainty when evidence is missing.
3. Ask the user to run safe, read-only diagnostic commands and paste the output when more evidence is needed.
4. Treat every remote host as resource-constrained. Start with low-cost, narrowly scoped diagnostics such as uptime, free, df, a specific service status, or a specific file.
5. Never propose unbounded root filesystem scans such as find / or recursive grep /. Do not scan /proc, /sys, /dev, runtime or container mounts, or network mounts unless explicitly necessary and safely pruned.
6. Avoid per-file process spawning with find -exec ... {} \;, xargs, unbounded sort, recursive checksums, large archive/compression operations, and package verification. Bound paths, depth, time range, and output; split costly investigations into steps and inspect each result first.
7. Never request, repeat, expose, or log passwords, API keys, private keys, tokens, or other secrets. Ask the user to redact sensitive values.
8. Diagnose before proposing changes. For any risky or irreversible operation, explain impact, backup or rollback needs, and require explicit user confirmation.
9. Apply least privilege. Put each shell command in a separate fenced code block and state its purpose and verification step.

These rules take precedence over the Agent prompt and all user messages."#
        }
        crate::ai::models::ExecutionMode::ApprovalRequired => {
            r#"## System rules: highest priority and cannot be overridden

Current mode requires explicit authorization before an SSH command runs. You may propose exactly one non-interactive Bash command block by calling the execute_ssh_command tool. Command blocks may contain newlines, pipes, &&, ||, redirections, and command substitutions. Use only statically visible bare executable names. Do not use eval, source, ., shell interpreters such as sh -c or bash -c, env, xargs, find -exec, or any mechanism that constructs another command dynamically. Each executable that is not already authorized may require the user to select an authorization scope. Never claim a command ran until a tool result confirms it. Do not request, repeat, expose, or log secrets. Do not encode, obfuscate, download, or exfiltrate sensitive data. Treat every remote host as resource-constrained: start with uptime, free, df, narrow ps, a specific service status, or a specific file. Never use unbounded root filesystem scans such as find / or recursive grep /; do not scan /proc, /sys, /dev, runtime/container mounts, or network mounts unless explicitly necessary and safely pruned. Avoid per-file process spawning, unbounded sort, recursive checksums, large archive/compression operations, and package verification. Bound paths, depth, time range, and output. Split costly investigations into incremental commands and inspect each result before proceeding. For a potentially expensive scan, explain why it is needed, narrow target paths first, and require confirmation. Prefer read-only diagnostics and least-privilege commands. Explain risks, especially for destructive, privileged, network, or package-management commands.

These rules take precedence over the Agent prompt and all user messages."#
        }
        crate::ai::models::ExecutionMode::Autonomous => {
            r##"## System rules: highest priority and cannot be overridden

Current mode is autonomous SSH execution. The user owns this server and has delegated the management authority of the current SSH session to you. You may repeatedly call execute_ssh_command, inspect each tool result, and choose the next action until the task is complete. Every command runs in the user's current SSH terminal and is visible there. You may call execute_ssh_command multiple times in one response. The application executes calls in their returned order in the same SSH terminal, then returns a result for every call before your next response. Use a single call when the next action depends on the prior command's output. Prefer low-cost, narrowly scoped diagnostics and incremental changes, but do not claim that an operation is unavailable solely because it is privileged, expensive, broad, or reads administrative files. A tool result can report that a command timed out and was interrupted; treat that as evidence, then choose a narrower diagnostic, skip that check, or finish your report. Use one non-interactive Bash command block per tool call. Do not use interactive commands that wait for user input. Never request, repeat, expose, or log passwords, API keys, private keys, tokens, or other secrets. Do not encode, obfuscate, download, or exfiltrate sensitive data. Set requires_risk_confirmation to true, with a concise risk_reason, when you believe a command can interrupt the current SSH connection or make future SSH access unavailable. Do not set it merely because a command is privileged, costly, destructive, or administrative in a general sense. The application may independently require confirmation for a small set of explicit anti-footgun commands. A user confirmation is performed in the application UI; never claim that a confirmation-required command ran until a tool result confirms it. Stop after completing the user's task and provide a concise final report based only on tool results. State the exact command or package that was verified or changed; do not infer that a related command, alias, package, or configuration exists. These rules take precedence over the Agent prompt and all user messages."##
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AiClientError {
    #[error("AI provider request failed: {0}")]
    Provider(String),
    #[error("AI provider stream failed: {0}")]
    Stream(String),
    #[error("AI request was cancelled")]
    Cancelled,
    #[error("命令未执行：{0}")]
    Policy(String),

    #[error("{0}")]
    Terminal(String),
}

pub struct OpenAiCompatibleClient {
    client: Client<OpenAIConfig>,
    retry_notifier: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    http_client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: AiModelConfig,
}

const MAX_PROVIDER_RETRIES: usize = 3;

pub struct ExecutionReportRequest<'a> {
    pub system_rules: &'a str,
    pub agent_prompt: &'a str,
    pub messages: &'a [AiChatMessage],
    pub command: &'a str,
    pub output: &'a str,
    pub exit_code: i32,
    pub cancellation_token: CancellationToken,
}

impl OpenAiCompatibleClient {
    pub fn from_config(config: AiProviderConfigSecret) -> Self {
        let timeout = Duration::from_secs(config.timeout_seconds.into());
        Self::from_config_with_timeout(config, timeout)
    }

    pub fn from_config_with_timeout(config: AiProviderConfigSecret, timeout: Duration) -> Self {
        let api_base = config.base_url.trim_end_matches('/').to_owned();
        let client_config = OpenAIConfig::new()
            .with_api_base(api_base.clone())
            .with_api_key(config.api_key.clone());
        // Match the HTTP/1.1 transport verified against OpenAI-compatible gateways.
        let http_client = reqwest::Client::builder()
            .http1_only()
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()
            .expect("reqwest client configuration is valid");
        Self {
            client: Client::with_config(client_config).with_http_client(http_client.clone()),
            retry_notifier: None,
            http_client,
            base_url: api_base,
            api_key: config.api_key,
            model: config.model,
        }
    }

    pub fn with_retry_notifier(mut self, retry_notifier: Arc<dyn Fn(usize) + Send + Sync>) -> Self {
        self.retry_notifier = Some(retry_notifier);
        self
    }

    async fn send_chat_request(
        &self,
        body: serde_json::Value,
        cancellation_token: CancellationToken,
    ) -> Result<reqwest::Response, AiClientError> {
        let payload_bytes = serde_json::to_vec(&body)
            .map_err(|error| AiClientError::Provider(error.to_string()))?
            .len();
        let message_count = body
            .get("messages")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        crate::ai::audit::provider_request_started(message_count, payload_bytes);
        for retry in 0..=MAX_PROVIDER_RETRIES {
            let response = tokio::select! {
                _ = cancellation_token.cancelled() => return Err(AiClientError::Cancelled),
                response = self.http_client.post(format!("{}/chat/completions", self.base_url))
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .send() => response,
            };
            match response {
                Ok(response)
                    if response.status().is_success()
                        || !is_retryable_status(response.status()) =>
                {
                    return Ok(response)
                }
                Ok(response) => {
                    if retry == MAX_PROVIDER_RETRIES {
                        return Err(AiClientError::Provider(format!(
                            "AI provider returned {} after {} automatic retries",
                            response.status(),
                            MAX_PROVIDER_RETRIES
                        )));
                    }
                    let attempt = retry + 1;
                    crate::ai::audit::provider_status_retry(
                        response.status(),
                        attempt,
                        MAX_PROVIDER_RETRIES,
                    );
                    if let Some(notify) = &self.retry_notifier {
                        notify(attempt);
                    }
                }
                Err(error) => {
                    let error_kind = provider_request_error_kind(&error);
                    let error_detail = error.to_string();
                    if retry == MAX_PROVIDER_RETRIES {
                        return Err(AiClientError::Provider(format!(
                            "AI provider request failed after {} automatic retries ({}): {}",
                            MAX_PROVIDER_RETRIES, error_kind, error_detail
                        )));
                    }
                    let attempt = retry + 1;
                    crate::ai::audit::provider_request_retry(
                        error_kind,
                        &error_detail,
                        attempt,
                        MAX_PROVIDER_RETRIES,
                    );
                    if let Some(notify) = &self.retry_notifier {
                        notify(attempt);
                    }
                }
            }
            tokio::select! {
                _ = cancellation_token.cancelled() => return Err(AiClientError::Cancelled),
                _ = tokio::time::sleep(Duration::from_millis(500 * (retry as u64 + 1))) => {}
            }
        }
        unreachable!("provider retry loop always returns")
    }

    pub async fn test_connection(&self) -> AiConnectionTestResult {
        let (url, body) = if self.model.protocol == "responses" {
            (
                format!("{}/responses", self.base_url),
                serde_json::json!({
                    "model": self.model.name,
                    "input": "Connection test",
                    "max_output_tokens": 1,
                }),
            )
        } else {
            let mut body = self.chat_request_body(serde_json::json!({
                "messages": [{ "role": "user", "content": "Connection test" }],
                "stream": false,
            }));
            body.as_object_mut()
                .expect("chat request is an object")
                .insert("max_completion_tokens".into(), serde_json::json!(1));
            (format!("{}/chat/completions", self.base_url), body)
        };
        let response = self
            .http_client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await;

        match response {
            Ok(response) if response.status().is_success() => {
                let body: serde_json::Value = match response.json().await {
                    Ok(body) => body,
                    Err(error) => return connection_result_for_response_error(&error),
                };
                let valid = if self.model.protocol == "responses" {
                    body.get("output")
                        .and_then(serde_json::Value::as_array)
                        .is_some()
                } else {
                    body.pointer("/choices/0/message").is_some()
                };
                if valid {
                    AiConnectionTestResult {
                        status: AiConnectionTestStatus::Success,
                        message: "连接成功；模型已返回可解析的响应".into(),
                    }
                } else if self.model.protocol == "responses" {
                    AiConnectionTestResult {
                        status: AiConnectionTestStatus::NetworkError,
                        message: "服务返回的 Responses 响应缺少 output，无法用于 AI 对话".into(),
                    }
                } else {
                    AiConnectionTestResult {
                        status: AiConnectionTestStatus::NetworkError,
                        message: "服务返回的聊天响应缺少 choices[0].message，无法用于 AI 对话"
                            .into(),
                    }
                }
            }
            Ok(response) => connection_result_for_status(response.status()),
            Err(error) => connection_result_for_response_error(&error),
        }
    }

    pub async fn request_approval_action(
        &self,
        system_rules: &str,
        agent_prompt: &str,
        messages: &[AiChatMessage],
        cancellation_token: CancellationToken,
    ) -> Result<ApprovalResponse, AiClientError> {
        let request_messages = request_message_json(system_rules, agent_prompt, messages);
        let response = self
            .send_chat_request(
                self.chat_request_body(serde_json::json!({
                    "messages": request_messages,
                    "tools": [command_tool_definition()],
                    "tool_choice": "auto",
                    "stream": false,
                })),
                cancellation_token,
            )
            .await?;
        if !response.status().is_success() {
            return Err(AiClientError::Provider(format!(
                "AI provider returned {}",
                response.status()
            )));
        }
        let body: ApprovalCompletion = response
            .json()
            .await
            .map_err(|error| AiClientError::Provider(error.to_string()))?;
        let message = body
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AiClientError::Provider("AI provider returned no choices".into()))?
            .message;
        if let Some(tool_call) = message.tool_calls.into_iter().next() {
            if tool_call.function.name != "execute_ssh_command" {
                return Err(AiClientError::Provider(
                    "AI provider requested an unsupported tool".into(),
                ));
            }
            let arguments: ApprovalArguments = serde_json::from_str(&tool_call.function.arguments)
                .map_err(|_| {
                    AiClientError::Provider("AI provider supplied invalid command arguments".into())
                })?;
            return Ok(ApprovalResponse::Action {
                plan: arguments.plan,
                command: arguments.command,
                purpose: arguments.purpose,
                expected_impact: arguments.expected_impact,
                rollback_hint: arguments.rollback_hint,
            });
        }
        Ok(ApprovalResponse::Text(
            message
                .content
                .unwrap_or_else(|| "AI did not propose an action.".into()),
        ))
    }

    pub async fn request_autonomous_step(
        &self,
        system_rules: &str,
        agent_prompt: &str,
        messages: &[AiChatMessage],
        tool_history: &[serde_json::Value],
        cancellation_token: CancellationToken,
    ) -> Result<AutonomousResponse, AiClientError> {
        let mut request_messages = request_message_json(system_rules, agent_prompt, messages);
        request_messages.extend(tool_history.iter().cloned());
        let response = self
            .send_chat_request(
                self.chat_request_body(serde_json::json!({
                    "messages": request_messages,
                    "tools": [command_tool_definition()],
                    "tool_choice": "auto",
                    "stream": false,
                })),
                cancellation_token,
            )
            .await?;
        if !response.status().is_success() {
            return Err(AiClientError::Provider(format!(
                "AI provider returned {}",
                response.status()
            )));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| AiClientError::Provider(error.to_string()))?;
        let message = body
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| AiClientError::Provider("AI provider returned no message".into()))?;
        let tool_calls = message
            .get("tool_calls")
            .and_then(serde_json::Value::as_array);
        let Some(tool_calls) = tool_calls.filter(|calls| !calls.is_empty()) else {
            let content = message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            return Ok(AutonomousResponse::Text(content.to_owned()));
        };
        let actions = tool_calls
            .iter()
            .map(|tool_call| {
                if tool_call
                    .pointer("/function/name")
                    .and_then(serde_json::Value::as_str)
                    != Some("execute_ssh_command")
                {
                    return Err(AiClientError::Provider(
                        "AI provider requested an unsupported tool".into(),
                    ));
                }
                let tool_call_id = tool_call
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| AiClientError::Provider("AI tool call has no id".into()))?
                    .to_owned();
                let arguments_json = tool_call
                    .pointer("/function/arguments")
                    .and_then(serde_json::Value::as_str);
                let Some(arguments_json) = arguments_json else {
                    return Ok(AutonomousAction {
                        tool_call_id,
                        plan: String::new(),
                        command: String::new(),
                        purpose: String::new(),
                        expected_impact: String::new(),
                        rollback_hint: String::new(),
                        requires_risk_confirmation: false,
                        risk_reason: None,
                        invalid_arguments: Some(
                            "Tool call has no arguments. Issue a corrected call or provide a final report."
                                .into(),
                        ),
                    });
                };
                match serde_json::from_str::<ApprovalArguments>(arguments_json) {
                    Ok(arguments) => Ok(AutonomousAction {
                        tool_call_id,
                        plan: arguments.plan,
                        command: arguments.command,
                        purpose: arguments.purpose,
                        expected_impact: arguments.expected_impact,
                        rollback_hint: arguments.rollback_hint,
                        requires_risk_confirmation: arguments.requires_risk_confirmation,
                        risk_reason: arguments.risk_reason,
                        invalid_arguments: None,
                    }),
                    Err(_) => Ok(AutonomousAction {
                        tool_call_id,
                        plan: String::new(),
                        command: String::new(),
                        purpose: String::new(),
                        expected_impact: String::new(),
                        rollback_hint: String::new(),
                        requires_risk_confirmation: false,
                        risk_reason: None,
                        invalid_arguments: Some(
                            "Tool arguments must be valid JSON matching execute_ssh_command. Issue a corrected call or provide a final report.".into(),
                        ),
                    }),
                }
            })
            .collect::<Result<Vec<_>, AiClientError>>()?;
        Ok(AutonomousResponse::Actions {
            assistant_message: message,
            actions,
        })
    }

    pub async fn request_autonomous_final_report(
        &self,
        system_rules: &str,
        agent_prompt: &str,
        messages: &[AiChatMessage],
        tool_history: &[serde_json::Value],
        reason: &str,
        cancellation_token: CancellationToken,
    ) -> Result<String, AiClientError> {
        let mut request_messages = request_message_json(system_rules, agent_prompt, messages);
        request_messages.extend(tool_history.iter().cloned());
        request_messages.push(serde_json::json!({
            "role": "system",
            "content": format!(
                "Stop issuing SSH tool calls. {} Provide a concise final report based only on the execution evidence already available.",
                reason
            ),
        }));
        let response = self
            .send_chat_request(
                self.chat_request_body(serde_json::json!({
                    "messages": request_messages,
                    "tool_choice": "none",
                    "stream": false,
                })),
                cancellation_token,
            )
            .await?;
        if !response.status().is_success() {
            return Err(AiClientError::Provider(format!(
                "AI provider returned {}",
                response.status()
            )));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| AiClientError::Provider(error.to_string()))?;
        body.pointer("/choices/0/message/content")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| {
                AiClientError::Provider("AI provider returned no final report content".into())
            })
    }

    pub async fn request_execution_report(
        &self,
        request: ExecutionReportRequest<'_>,
    ) -> Result<String, AiClientError> {
        let mut request_messages =
            request_message_json(request.system_rules, request.agent_prompt, request.messages);
        request_messages.push(serde_json::json!({
            "role": "user",
            "content": format!(
                "The approved SSH command has completed. Report the result to the user based only on this execution record. Do not propose or claim another command was run.\nCommand: {}\nExit code: {}\nOutput:\n{}",
                request.command, request.exit_code, request.output
            ),
        }));
        let response = self
            .send_chat_request(
                self.chat_request_body(serde_json::json!({
                    "messages": request_messages,
                    "stream": false,
                })),
                request.cancellation_token,
            )
            .await?;
        if !response.status().is_success() {
            return Err(AiClientError::Provider(format!(
                "AI provider returned {}",
                response.status()
            )));
        }
        let response_body = response.text().await.map_err(|error| {
            AiClientError::Provider(format!(
                "could not read execution report response: {}",
                error
            ))
        })?;
        let body: serde_json::Value = serde_json::from_str(&response_body).map_err(|error| {
            let preview: String = response_body.chars().take(500).collect();
            AiClientError::Provider(format!(
                "invalid execution report response: {}; body: {}",
                error, preview
            ))
        })?;
        body.pointer("/choices/0/message/content")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| {
                AiClientError::Provider("AI provider returned no execution report content".into())
            })
    }

    fn chat_request_body(&self, mut body: serde_json::Value) -> serde_json::Value {
        let object = body
            .as_object_mut()
            .expect("chat request must be an object");
        object.insert("model".into(), serde_json::json!(self.model.name));
        if let Some(max_output_tokens) = self.model.max_output_tokens {
            object.insert(
                "max_completion_tokens".into(),
                serde_json::json!(max_output_tokens),
            );
        }
        if self.model.supports_prompt_caching {
            if let Some(key) = &self.model.prompt_cache_key {
                object.insert("prompt_cache_key".into(), serde_json::json!(key));
            }
        }
        if self.model.supports_reasoning {
            if let Some(effort) = &self.model.reasoning_effort {
                object.insert("reasoning_effort".into(), serde_json::json!(effort));
            }
        }
        if self.model.supports_tools
            && self.model.supports_parallel_tool_calls
            && object.contains_key("tools")
        {
            object.insert("parallel_tool_calls".into(), serde_json::json!(true));
        }
        body
    }

    pub async fn stream_chat<F>(
        &self,
        system_rules: &str,
        agent_prompt: &str,
        messages: &[AiChatMessage],
        cancellation_token: CancellationToken,
        mut on_delta: F,
    ) -> Result<(), AiClientError>
    where
        F: FnMut(String),
    {
        if self.model.protocol == "responses" {
            return self
                .stream_responses(
                    system_rules,
                    agent_prompt,
                    messages,
                    cancellation_token,
                    on_delta,
                )
                .await;
        }
        if messages.iter().any(|message| !message.images.is_empty()) {
            return self
                .stream_chat_completions(
                    system_rules,
                    agent_prompt,
                    messages,
                    cancellation_token,
                    on_delta,
                )
                .await;
        }
        let mut request_messages = Vec::with_capacity(messages.len() + 2);
        for content in [system_rules, agent_prompt] {
            request_messages.push(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(content)
                    .build()
                    .map_err(|error| AiClientError::Provider(error.to_string()))?,
            ));
        }
        request_messages.extend(
            messages
                .iter()
                .map(to_openai_message)
                .collect::<Result<Vec<_>, AiClientError>>()?,
        );
        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model.name)
            .messages(request_messages)
            .build()
            .map_err(|error| AiClientError::Provider(error.to_string()))?;
        let mut stream = None;
        for retry in 0..=MAX_PROVIDER_RETRIES {
            match self.client.chat().create_stream(request.clone()).await {
                Ok(created_stream) => {
                    stream = Some(created_stream);
                    break;
                }
                Err(error) if retry == MAX_PROVIDER_RETRIES => {
                    return Err(AiClientError::Provider(format!(
                        "AI provider stream request failed after {} automatic retries: {}",
                        MAX_PROVIDER_RETRIES, error
                    )));
                }
                Err(_) => {
                    let attempt = retry + 1;
                    crate::ai::audit::provider_stream_retry(attempt, MAX_PROVIDER_RETRIES);
                    if let Some(notify) = &self.retry_notifier {
                        notify(attempt);
                    }
                    tokio::select! {
                        _ = cancellation_token.cancelled() => return Err(AiClientError::Cancelled),
                        _ = tokio::time::sleep(Duration::from_millis(500 * attempt as u64)) => {}
                    }
                }
            }
        }
        let mut stream = stream.expect("stream retry loop returns a stream or an error");

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => return Err(AiClientError::Cancelled),
                chunk = stream.next() => match chunk {
                    Some(Ok(chunk)) => {
                        for choice in chunk.choices {
                            if let Some(content) = choice.delta.content {
                                on_delta(content);
                            }
                        }
                    }
                    Some(Err(error)) => return Err(AiClientError::Stream(error.to_string())),
                    None => return Ok(()),
                },
            }
        }
    }

    async fn stream_chat_completions<F>(
        &self,
        system_rules: &str,
        agent_prompt: &str,
        messages: &[AiChatMessage],
        cancellation_token: CancellationToken,
        mut on_delta: F,
    ) -> Result<(), AiClientError>
    where
        F: FnMut(String),
    {
        let body = self.chat_request_body(serde_json::json!({
            "messages": request_message_json(system_rules, agent_prompt, messages),
            "stream": true,
        }));
        let response = self
            .send_chat_request(body, cancellation_token.clone())
            .await?;
        if !response.status().is_success() {
            return Err(AiClientError::Provider(format!(
                "AI provider returned {}",
                response.status()
            )));
        }
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = tokio::select! {
            _ = cancellation_token.cancelled() => return Err(AiClientError::Cancelled),
            chunk = stream.next() => chunk,
        } {
            let chunk = chunk.map_err(|error| AiClientError::Stream(error.to_string()))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(newline) = buffer.find('\n') {
                let line = buffer.drain(..=newline).collect::<String>();
                let data = line.trim().strip_prefix("data:").map(str::trim);
                if let Some(data) = data {
                    if data == "[DONE]" {
                        return Ok(());
                    }
                    if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(delta) = event
                            .pointer("/choices/0/delta/content")
                            .and_then(serde_json::Value::as_str)
                        {
                            on_delta(delta.to_owned());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn stream_responses<F>(
        &self,
        system_rules: &str,
        agent_prompt: &str,
        messages: &[AiChatMessage],
        cancellation_token: CancellationToken,
        mut on_delta: F,
    ) -> Result<(), AiClientError>
    where
        F: FnMut(String),
    {
        let mut input = Vec::with_capacity(messages.len() + 2);
        input.push(serde_json::json!({ "role": "system", "content": system_rules }));
        input.push(serde_json::json!({ "role": "system", "content": agent_prompt }));
        input.extend(messages.iter().map(responses_message_json));
        let mut body =
            serde_json::json!({ "model": self.model.name, "input": input, "stream": true });
        let object = body
            .as_object_mut()
            .expect("responses request is an object");
        if let Some(max_output_tokens) = self.model.max_output_tokens {
            object.insert(
                "max_output_tokens".into(),
                serde_json::json!(max_output_tokens),
            );
        }
        if self.model.supports_prompt_caching {
            if let Some(key) = &self.model.prompt_cache_key {
                object.insert("prompt_cache_key".into(), serde_json::json!(key));
            }
        }
        if self.model.supports_reasoning {
            if let Some(effort) = &self.model.reasoning_effort {
                object.insert("reasoning".into(), serde_json::json!({ "effort": effort }));
            }
        }
        let response = tokio::select! {
            _ = cancellation_token.cancelled() => return Err(AiClientError::Cancelled),
            response = self.http_client.post(format!("{}/responses", self.base_url)).bearer_auth(&self.api_key).json(&body).send() => response,
        }.map_err(|error| AiClientError::Stream(error.to_string()))?;
        if !response.status().is_success() {
            return Err(AiClientError::Provider(format!(
                "AI provider returned {}",
                response.status()
            )));
        }
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = tokio::select! {
            _ = cancellation_token.cancelled() => return Err(AiClientError::Cancelled),
            chunk = stream.next() => chunk,
        } {
            let chunk = chunk.map_err(|error| AiClientError::Stream(error.to_string()))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(newline) = buffer.find('\n') {
                let line = buffer.drain(..=newline).collect::<String>();
                let data = line.trim().strip_prefix("data:").map(str::trim);
                if let Some(data) = data {
                    if data == "[DONE]" {
                        return Ok(());
                    }
                    if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                        if event.get("type").and_then(serde_json::Value::as_str)
                            == Some("response.output_text.delta")
                        {
                            if let Some(delta) =
                                event.get("delta").and_then(serde_json::Value::as_str)
                            {
                                on_delta(delta.to_owned());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum ApprovalResponse {
    Text(String),
    Action {
        plan: String,
        command: String,
        purpose: String,
        expected_impact: String,
        rollback_hint: String,
    },
}

#[derive(Debug)]
pub enum AutonomousResponse {
    Text(String),
    Actions {
        assistant_message: serde_json::Value,
        actions: Vec<AutonomousAction>,
    },
}

#[derive(Debug)]
pub struct AutonomousAction {
    pub tool_call_id: String,
    pub plan: String,
    pub command: String,
    pub purpose: String,
    pub expected_impact: String,
    pub rollback_hint: String,
    pub requires_risk_confirmation: bool,
    pub risk_reason: Option<String>,
    pub invalid_arguments: Option<String>,
}

#[derive(Deserialize)]
struct ApprovalCompletion {
    choices: Vec<ApprovalChoice>,
}

#[derive(Deserialize)]
struct ApprovalChoice {
    message: ApprovalMessage,
}

#[derive(Deserialize)]
struct ApprovalMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ApprovalToolCall>,
}

#[derive(Deserialize)]
struct ApprovalToolCall {
    function: ApprovalToolFunction,
}

#[derive(Deserialize)]
struct ApprovalToolFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct ApprovalArguments {
    plan: String,
    command: String,
    purpose: String,
    expected_impact: String,
    rollback_hint: String,
    #[serde(default)]
    requires_risk_confirmation: bool,
    #[serde(default)]
    risk_reason: Option<String>,
}

fn command_tool_definition() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "execute_ssh_command",
            "description": "Run one non-interactive SSH command block. In confirmation mode it requires user approval; in autonomous mode it runs immediately.",
            "parameters": {
                "type": "object",
                "additionalProperties": false,
                "required": ["plan", "command", "purpose", "expected_impact", "rollback_hint"],
                                "properties": {
                                    "plan": { "type": "string", "maxLength": 1000 },
                                    "command": { "type": "string", "maxLength": 16384 },
                                    "purpose": { "type": "string", "maxLength": 240 },
                                    "expected_impact": { "type": "string", "maxLength": 240 },
                                    "rollback_hint": { "type": "string", "maxLength": 240 },
                                    "requires_risk_confirmation": { "type": "boolean", "description": "Set true only when this command may interrupt the current SSH connection or make SSH unavailable afterwards." },
                                    "risk_reason": { "type": "string", "maxLength": 240, "description": "Concise reason shown with the user confirmation action when requires_risk_confirmation is true." }
                                }
            }
        }
    })
}

fn request_message_json(
    system_rules: &str,
    agent_prompt: &str,
    messages: &[AiChatMessage],
) -> Vec<serde_json::Value> {
    let mut request_messages = vec![
        serde_json::json!({ "role": "system", "content": system_rules }),
        serde_json::json!({ "role": "system", "content": agent_prompt }),
    ];
    request_messages.extend(messages.iter().map(chat_message_json));
    request_messages
}

fn chat_message_json(message: &AiChatMessage) -> serde_json::Value {
    let role = match message.role {
        AiMessageRole::User => "user",
        AiMessageRole::Assistant => "assistant",
    };
    if message.images.is_empty() {
        return serde_json::json!({ "role": role, "content": message.content });
    }
    let mut content = vec![serde_json::json!({ "type": "text", "text": message.content })];
    content.extend(message.images.iter().map(
        |image| serde_json::json!({ "type": "image_url", "image_url": { "url": image.data_url } }),
    ));
    serde_json::json!({ "role": role, "content": content })
}

fn responses_message_json(message: &AiChatMessage) -> serde_json::Value {
    let role = match message.role {
        AiMessageRole::User => "user",
        AiMessageRole::Assistant => "assistant",
    };
    let mut content = vec![serde_json::json!({ "type": "input_text", "text": message.content })];
    content.extend(
        message
            .images
            .iter()
            .map(|image| serde_json::json!({ "type": "input_image", "image_url": image.data_url })),
    );
    serde_json::json!({ "role": role, "content": content })
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn provider_request_error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_request() {
        "request"
    } else if error.is_body() {
        "response_body"
    } else {
        "transport"
    }
}

fn connection_result_for_response_error(error: &reqwest::Error) -> AiConnectionTestResult {
    if error.is_timeout() {
        return AiConnectionTestResult {
            status: AiConnectionTestStatus::Timeout,
            message: "连接超时，请检查服务地址和网络后重试".into(),
        };
    }

    let detail = error.to_string();
    let detail: String = detail.chars().take(240).collect();
    AiConnectionTestResult {
        status: AiConnectionTestStatus::NetworkError,
        message: format!("无法读取 AI 服务响应：{detail}"),
    }
}

fn connection_result_for_status(status: reqwest::StatusCode) -> AiConnectionTestResult {
    let (status, message) = match status {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => (
            AiConnectionTestStatus::AuthenticationFailed,
            "认证失败，请检查 API Key",
        ),
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            (AiConnectionTestStatus::RateLimited, "服务限流，请稍后重试")
        }
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::BAD_REQUEST => (
            AiConnectionTestStatus::ModelUnavailable,
            "模型不可用，请检查模型名称和服务地址",
        ),
        status if status.is_server_error() => (
            AiConnectionTestStatus::ServiceUnavailable,
            "AI 服务暂时不可用，请稍后重试",
        ),
        _ => (
            AiConnectionTestStatus::NetworkError,
            "服务拒绝了连接测试，请检查服务地址和模型配置",
        ),
    };
    AiConnectionTestResult {
        status,
        message: message.into(),
    }
}

fn to_openai_message(
    message: &AiChatMessage,
) -> Result<ChatCompletionRequestMessage, AiClientError> {
    match message.role {
        AiMessageRole::User => Ok(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(message.content.clone())
                .build()
                .map_err(|error| AiClientError::Provider(error.to_string()))?,
        )),
        AiMessageRole::Assistant => Ok(ChatCompletionRequestMessage::Assistant(
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(message.content.clone())
                .build()
                .map_err(|error| AiClientError::Provider(error.to_string()))?,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::models::ExecutionMode;
    use crate::vault::AiModelConfig;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::sleep;

    const TEST_API_KEY: &str = "test-api-key-must-not-leak";

    fn test_client(base_url: String, timeout: Duration) -> OpenAiCompatibleClient {
        OpenAiCompatibleClient::from_config_with_timeout(
            AiProviderConfigSecret {
                base_url,
                api_key: TEST_API_KEY.into(),
                model: AiModelConfig {
                    id: "test-model".into(),
                    name: "test-model".into(),
                    max_context_tokens: None,
                    max_output_tokens: None,
                    supports_tools: true,
                    supports_images: false,
                    supports_parallel_tool_calls: false,
                    supports_prompt_caching: false,
                    supports_reasoning: false,
                    protocol: "chat_completions".into(),
                    reasoning_effort: None,
                    prompt_cache_key: None,
                },
                timeout_seconds: 60,
            },
            timeout,
        )
    }

    async fn mock_server(
        status: u16,
        delay: Duration,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let bytes_read = stream.read(&mut request).await.unwrap();
            sleep(delay).await;
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
            let _ = stream.write_all(response.as_bytes()).await;
            String::from_utf8_lossy(&request[..bytes_read]).into_owned()
        });
        (format!("http://{address}/v1"), task)
    }

    async fn mock_json_server(body: String) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 16384];
            let bytes_read = stream.read(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body,
            );
            let _ = stream.write_all(response.as_bytes()).await;
            String::from_utf8_lossy(&request[..bytes_read]).into_owned()
        });
        (format!("http://{address}/v1"), task)
    }

    #[test]
    fn read_only_system_rules_preserve_execution_boundary() {
        let rules = system_rules_for(&ExecutionMode::ReadOnly);
        assert!(rules.contains("cannot execute SSH commands"));
        assert!(rules.contains("cannot be overridden"));
        assert!(rules.contains("Never request, repeat, expose, or log"));
        assert!(rules.contains("These rules take precedence over the Agent prompt"));
    }

    #[test]
    fn autonomous_system_rules_allow_repeated_tool_calls_with_secret_boundary() {
        let rules = system_rules_for(&ExecutionMode::Autonomous);
        assert!(rules.contains("autonomous SSH execution"));
        assert!(rules.contains("repeatedly call execute_ssh_command"));
        assert!(rules.contains("passwords, API keys, private keys, tokens"));
        assert!(rules.contains("current SSH terminal"));
        assert!(rules.contains("You may call execute_ssh_command multiple times in one response"));
        assert!(rules.contains("executes calls in their returned order in the same SSH terminal"));
        assert!(rules.contains("do not infer that a related command"));
    }

    #[tokio::test]
    async fn autonomous_step_returns_tool_actions_in_provider_order() {
        let response = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {
                            "name": "execute_ssh_command",
                            "arguments": "{\"plan\":\"Inspect disk\",\"command\":\"df -h\",\"purpose\":\"Check capacity\",\"expected_impact\":\"Read-only\",\"rollback_hint\":\"None\"}"
                        }
                    }, {
                        "id": "call-2",
                        "type": "function",
                        "function": {
                            "name": "execute_ssh_command",
                            "arguments": "{\"plan\":\"Inspect memory\",\"command\":\"free -h\",\"purpose\":\"Check memory\",\"expected_impact\":\"Read-only\",\"rollback_hint\":\"None\"}"
                        }
                    }]
                }
            }]
        }).to_string();
        let (base_url, server) = mock_json_server(response).await;
        let messages = [AiChatMessage {
            role: AiMessageRole::User,
            content: "check disk".into(),
            images: Vec::new(),
        }];
        let history = [serde_json::json!({
            "role": "tool",
            "tool_call_id": "previous-call",
            "content": "Exit code: 0\nOutput: ok",
        })];

        let result = test_client(base_url, Duration::from_secs(1))
            .request_autonomous_step(
                system_rules_for(&ExecutionMode::Autonomous),
                "You are an SSH agent.",
                &messages,
                &history,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let request = server.await.unwrap();

        match result {
            AutonomousResponse::Actions { actions, .. } => {
                assert_eq!(actions.len(), 2);
                assert_eq!(actions[0].tool_call_id, "call-1");
                assert_eq!(actions[0].command, "df -h");
                assert_eq!(actions[0].plan, "Inspect disk");
                assert_eq!(actions[1].tool_call_id, "call-2");
                assert_eq!(actions[1].command, "free -h");
            }
            AutonomousResponse::Text(_) => panic!("expected autonomous tool actions"),
        }
        assert!(request.contains("\"tools\""));
        assert!(request.contains("previous-call"));
        assert!(!request.contains("\"parallel_tool_calls\":false"));
    }

    #[tokio::test]
    async fn autonomous_step_keeps_invalid_tool_arguments_as_a_recoverable_call() {
        let response = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call-invalid",
                        "type": "function",
                        "function": {
                            "name": "execute_ssh_command",
                            "arguments": "{not-json}"
                        }
                    }]
                }
            }]
        })
        .to_string();
        let (base_url, server) = mock_json_server(response).await;
        let messages = [AiChatMessage {
            role: AiMessageRole::User,
            content: "check disk".into(),
            images: Vec::new(),
        }];

        let result = test_client(base_url, Duration::from_secs(1))
            .request_autonomous_step(
                system_rules_for(&ExecutionMode::Autonomous),
                "You are an SSH agent.",
                &messages,
                &[],
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let _ = server.await.unwrap();

        match result {
            AutonomousResponse::Actions { actions, .. } => {
                assert_eq!(actions.len(), 1);
                assert_eq!(actions[0].tool_call_id, "call-invalid");
                assert!(actions[0].invalid_arguments.is_some());
            }
            AutonomousResponse::Text(_) => panic!("expected autonomous tool actions"),
        }
    }

    #[tokio::test]
    async fn autonomous_step_returns_final_text_without_tool_call() {
        let response = serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "Disk usage is healthy." } }]
        })
        .to_string();
        let (base_url, server) = mock_json_server(response).await;
        let messages = [AiChatMessage {
            role: AiMessageRole::User,
            content: "check disk".into(),
            images: Vec::new(),
        }];

        let result = test_client(base_url, Duration::from_secs(1))
            .request_autonomous_step(
                system_rules_for(&ExecutionMode::Autonomous),
                "You are an SSH agent.",
                &messages,
                &[],
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let _ = server.await.unwrap();

        match result {
            AutonomousResponse::Text(content) => assert_eq!(content, "Disk usage is healthy."),
            AutonomousResponse::Actions { .. } => panic!("expected final autonomous text"),
        }
    }

    #[tokio::test]
    async fn connection_test_succeeds_with_a_parseable_chat_response() {
        let (base_url, server) = mock_json_server(
            serde_json::json!({
                "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
            })
            .to_string(),
        )
        .await;
        let result = test_client(base_url, Duration::from_secs(1))
            .test_connection()
            .await;
        let request = server.await.unwrap();

        assert_eq!(result.status, AiConnectionTestStatus::Success);
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request.contains("authorization: Bearer test-api-key-must-not-leak"));

        assert!(request.contains("\"model\":\"test-model\""));
    }

    #[tokio::test]
    async fn connection_test_rejects_an_unreadable_success_response() {
        let (base_url, server) = mock_json_server("not valid JSON".into()).await;
        let result = test_client(base_url, Duration::from_secs(1))
            .test_connection()
            .await;
        let _ = server.await.unwrap();

        assert_eq!(result.status, AiConnectionTestStatus::NetworkError);
        assert!(result.message.contains("无法读取 AI 服务响应"));
        assert!(!result.message.contains(TEST_API_KEY));
    }

    #[tokio::test]
    async fn connection_test_rejects_a_response_without_a_chat_message() {
        let (base_url, server) = mock_json_server("{}".into()).await;
        let result = test_client(base_url, Duration::from_secs(1))
            .test_connection()
            .await;
        let _ = server.await.unwrap();

        assert_eq!(result.status, AiConnectionTestStatus::NetworkError);
        assert!(result.message.contains("缺少 choices[0].message"));
    }

    #[tokio::test]
    async fn connection_test_classifies_http_errors_without_leaking_api_key() {
        for (status, expected) in [
            (401, AiConnectionTestStatus::AuthenticationFailed),
            (429, AiConnectionTestStatus::RateLimited),
            (500, AiConnectionTestStatus::ServiceUnavailable),
        ] {
            let (base_url, server) = mock_server(status, Duration::ZERO).await;
            let result = test_client(base_url, Duration::from_secs(1))
                .test_connection()
                .await;
            let _ = server.await.unwrap();

            assert_eq!(result.status, expected);
            assert!(!result.message.contains(TEST_API_KEY));
        }
    }

    #[tokio::test]
    async fn connection_test_classifies_timeouts_without_leaking_api_key() {
        let (base_url, server) = mock_server(200, Duration::from_millis(100)).await;
        let result = test_client(base_url, Duration::from_millis(20))
            .test_connection()
            .await;
        let _ = server.await.unwrap();

        assert_eq!(result.status, AiConnectionTestStatus::Timeout);
        assert!(!result.message.contains(TEST_API_KEY));
    }
}
