use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    Password,
    Key,
    Certificate,
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
    #[serde(with = "opt_hex_bytes")]
    pub credential: Option<Vec<u8>>,
    #[serde(with = "opt_hex_bytes")]
    pub private_key: Option<Vec<u8>>,
    #[serde(with = "opt_hex_bytes")]
    pub cert_data: Option<Vec<u8>>,
    pub key_id: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
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
    #[serde(with = "hex_bytes")]
    pub private_key: Vec<u8>,
    #[serde(with = "opt_hex_bytes")]
    pub cert_data: Option<Vec<u8>>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshKeyView {
    pub id: String,
    pub name: String,
    pub key_type: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    pub key_type: String,
    pub private_key: String,
    pub cert_data: Option<String>,
}

#[derive(Debug, Zeroize)]
pub struct DecryptedCredential {
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub cert_data: Option<String>,
}

mod hex_bytes {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

mod opt_hex_bytes {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match bytes {
            Some(b) => serializer.serialize_str(&hex::encode(b)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<String> = Option::deserialize(deserializer)?;
        match s {
            Some(hex_str) => hex::decode(&hex_str)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}
