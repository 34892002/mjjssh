use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const DEFAULT_TERMINAL_TYPE: &str = "xterm-256color";
pub const DEFAULT_FONT_SIZE: u8 = 14;
pub const DEFAULT_FONT_FAMILY: &str =
    "Cascadia Code, Fira Code, JetBrains Mono, Consolas, monospace";
pub const DEFAULT_SCROLLBACK_LINES: u32 = 5_000;
pub const DEFAULT_CONNECT_TIMEOUT_SECONDS: u32 = 30;
pub const DEFAULT_KEEPALIVE_INTERVAL_SECONDS: u32 = 60;

fn default_font_family() -> String {
    DEFAULT_FONT_FAMILY.into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub terminal_type: String,
    pub font_size: u8,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    pub scrollback_lines: u32,
    pub backspace_sends: String,
    pub alt_sends_escape: bool,
    pub connect_timeout_seconds: u32,
    pub keepalive_interval_seconds: u32,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            terminal_type: DEFAULT_TERMINAL_TYPE.into(),
            font_size: DEFAULT_FONT_SIZE,
            font_family: default_font_family(),
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
            backspace_sends: "del".into(),
            alt_sends_escape: true,
            connect_timeout_seconds: DEFAULT_CONNECT_TIMEOUT_SECONDS,
            keepalive_interval_seconds: DEFAULT_KEEPALIVE_INTERVAL_SECONDS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub minimize_to_tray_on_close: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScriptRiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub command: String,
    pub risk_level: ScriptRiskLevel,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScriptRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub command: String,
    #[serde(default = "default_script_risk_level")]
    pub risk_level: ScriptRiskLevel,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScriptRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub command: Option<String>,
    pub risk_level: Option<ScriptRiskLevel>,
}

fn default_script_risk_level() -> ScriptRiskLevel {
    ScriptRiskLevel::Medium
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    Password,
    Key,
    Certificate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Socks5ProxyAuthType {
    None,
    Password,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Socks5Proxy {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub auth_type: Socks5ProxyAuthType,
    pub username: Option<String>,
    pub password: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Socks5ProxyView {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub auth_type: Socks5ProxyAuthType,
    pub username: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSocks5ProxyRequest {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub auth_type: Socks5ProxyAuthType,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSocks5ProxyRequest {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub auth_type: Socks5ProxyAuthType,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::Password => write!(f, "password"),
            AuthType::Key => write!(f, "key"),
            AuthType::Certificate => write!(f, "certificate"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub credential: Option<String>,
    pub key_id: Option<String>,
    #[serde(default)]
    pub proxy_id: Option<String>,
    pub group_name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub os: Option<String>,
    pub location: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub auth_type: AuthType,
    pub credential: Option<String>,
    pub key_id: Option<String>,
    pub proxy_id: Option<String>,
    pub group_name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub os: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub auth_type: Option<AuthType>,
    pub credential: Option<String>,
    pub key_id: Option<String>,
    pub proxy_id: Option<String>,
    #[serde(default)]
    pub clear_proxy: bool,
    pub group_name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub os: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshProfileView {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub key_id: Option<String>,
    pub proxy_id: Option<String>,
    pub group_name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub os: Option<String>,
    pub location: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&SshProfile> for SshProfileView {
    fn from(p: &SshProfile) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            host: p.host.clone(),
            port: p.port,
            username: p.username.clone(),
            auth_type: p.auth_type.clone(),
            key_id: p.key_id.clone(),
            proxy_id: p.proxy_id.clone(),
            group_name: p.group_name.clone(),
            icon: p.icon.clone(),
            color: p.color.clone(),
            os: p.os.clone(),
            location: p.location.clone(),
            created_at: p.created_at.clone(),
            updated_at: p.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelConfig {
    pub id: String,
    pub name: String,
    pub max_context_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_parallel_tool_calls: bool,
    #[serde(default)]
    pub supports_prompt_caching: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default = "default_ai_model_protocol")]
    pub protocol: String,
    pub reasoning_effort: Option<String>,
    pub prompt_cache_key: Option<String>,
}

fn default_ai_model_protocol() -> String {
    "chat_completions".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAiProviderConfigRequest {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_seconds: u32,
    #[serde(default)]
    pub models: Vec<AiModelConfig>,
    #[serde(default)]
    pub active_model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfigView {
    pub configured: bool,
    pub provider_type: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub models: Vec<AiModelConfig>,
    pub active_model_id: Option<String>,
}

pub struct AiProviderConfigSecret {
    pub base_url: String,
    pub api_key: String,
    pub model: AiModelConfig,
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgentConfig {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAiAgentConfigRequest {
    pub id: Option<String>,
    pub name: String,
    pub prompt: String,
}

// ==================== SSH Keys ====================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshKey {
    pub id: String,
    pub name: String,
    pub key_type: String,
    #[serde(default)]
    pub algorithm: String,
    pub private_key: String,
    pub cert_data: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshKeyView {
    pub id: String,
    pub name: String,
    pub key_type: String,
    pub algorithm: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    pub key_type: String,
    #[serde(default)]
    pub algorithm: Option<String>,
    pub private_key: String,
    pub cert_data: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateSshKeyRequest {
    pub name: String,
    pub algorithm: SshKeyAlgorithm,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SshKeyAlgorithm {
    Ed25519,
    Rsa,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateSshKeyResult {
    pub key: SshKeyView,
    pub public_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTerminalSettingsRequest {
    pub terminal_type: String,
    pub font_size: u8,
    pub font_family: String,
    pub scrollback_lines: u32,
    pub backspace_sends: String,
    pub alt_sends_escape: bool,
    pub connect_timeout_seconds: u32,
    pub keepalive_interval_seconds: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAppSettingsRequest {
    pub minimize_to_tray_on_close: bool,
}

#[derive(Debug, Zeroize)]
pub struct DecryptedCredential {
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub key_algorithm: Option<String>,
    pub cert_data: Option<String>,
}
