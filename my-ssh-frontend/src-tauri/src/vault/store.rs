use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ssh_key::{Certificate, PrivateKey};

use crate::local_security::{
    create_local_vault_key, decrypt_vault, encrypt_vault, existing_local_vault_key,
    LocalEncryptedVault, LocalSecurityError,
};

use super::models::{
    AiAgentConfig, AiModelConfig, AiProviderConfigSecret, AiProviderConfigView, AuthType,
    CreateKeyRequest, CreateProfileRequest, CreateScriptRequest, CreateSocks5ProxyRequest,
    DecryptedCredential, SaveAiAgentConfigRequest, SaveAiProviderConfigRequest,
    SaveTerminalSettingsRequest, Script, Socks5Proxy, Socks5ProxyAuthType, Socks5ProxyView, SshKey,
    SshKeyView, SshProfile, TerminalSettings, UpdateProfileRequest, UpdateScriptRequest,
    UpdateSocks5ProxyRequest,
};

const VAULT_FILE_NAME: &str = "vault.json";
const BACKUP_FILE_NAME: &str = "vault.json.bak";
const FORMAT_VERSION: u32 = 2;
const DEFAULT_AGENT_ID: &str = "mjj-agent";
const DEFAULT_AGENT_NAME: &str = "MJJ Agent";
const DEFAULT_AGENT_PROMPT: &str = r#"# Role: MJJ Agent (高级远程运维专家 & SSH 智能助手)

## 角色定位与核心能力
你是一位拥有十年以上经验的资深 Linux/Unix 运维专家。协助用户管理服务器、排查故障、部署服务并优化系统性能。你精通 Ubuntu、Debian、CentOS、Alpine、Nginx、Apache、Docker、Kubernetes、MySQL、PostgreSQL 及网络安全配置。

## 沟通方式
根据用户的技术水平调整表达。对初学者使用清晰的中文分步骤说明，并说明每步的预期结果；对有经验的运维人员先给出诊断结论，使用简洁准确的术语。使用 Markdown 标题、列表和代码块组织答案。"#;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Vault is not open")]
    NotInitialized,
    #[error("Vault storage error: {0}")]
    Storage(String),
    #[error("Vault local encryption error: {0}")]
    LocalSecurity(#[from] LocalSecurityError),
    #[error("Vault encryption key is unavailable in the system credential store")]
    LocalKeyUnavailable,
    #[error("Vault file is invalid: {0}")]
    InvalidFormat(String),
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
    #[error("Invalid AI configuration: {0}")]
    InvalidAiConfig(String),
    #[error("Invalid terminal settings: {0}")]
    InvalidTerminalSettings(String),
    #[error("Invalid SSH key configuration: {0}")]
    InvalidSshKeyConfig(String),
    #[error("AI Agent not found: {0}")]
    AiAgentNotFound(String),
    #[error("The default AI Agent cannot be deleted")]
    DefaultAiAgentCannotBeDeleted,
    #[error("Script not found: {0}")]
    ScriptNotFound(String),
    #[error("Invalid script: {0}")]
    InvalidScript(String),
    #[error("Proxy not found: {0}")]
    ProxyNotFound(String),
    #[error("Invalid SOCKS5 proxy: {0}")]
    InvalidProxy(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultDocument {
    format_version: u32,
    vault_id: String,
    revision: u64,
    updated_at: String,
    #[serde(default)]
    profiles: Vec<SshProfile>,
    #[serde(default)]
    ssh_keys: Vec<SshKey>,
    #[serde(default)]
    proxies: Vec<Socks5Proxy>,
    #[serde(default)]
    ai_provider_config: Option<StoredAiProviderConfig>,
    #[serde(default)]
    ai_agents: Vec<AiAgentConfig>,
    #[serde(default)]
    ai_executable_grants: Vec<StoredAiExecutableGrant>,
    #[serde(default)]
    scripts: Vec<Script>,
    #[serde(default)]
    terminal_settings: TerminalSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAiProviderConfig {
    provider_type: String,
    base_url: String,
    api_key: String,
    active_model_id: String,
    models: Vec<AiModelConfig>,
    timeout_seconds: u32,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAiExecutableGrant {
    executable_name: String,
    scope: String,
    scope_target: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
pub struct VaultSyncSnapshot {
    pub content: Vec<u8>,
    pub vault_id: String,
    pub revision: u64,
}

pub struct Vault {
    path: PathBuf,
    local_key: [u8; crate::local_security::LOCAL_KEY_LENGTH],
    document: Mutex<VaultDocument>,
}

impl Vault {
    pub fn open(app_dir: &Path) -> Result<Self, VaultError> {
        fs::create_dir_all(app_dir).map_err(io_error)?;
        let path = app_dir.join(VAULT_FILE_NAME);
        let key = if path.exists() {
            existing_local_vault_key()?.ok_or(VaultError::LocalKeyUnavailable)?
        } else {
            create_local_vault_key()?
        };
        Self::open_with_key(app_dir, key)
    }

    fn open_with_key(
        app_dir: &Path,
        local_key: [u8; crate::local_security::LOCAL_KEY_LENGTH],
    ) -> Result<Self, VaultError> {
        fs::create_dir_all(app_dir).map_err(io_error)?;
        let path = app_dir.join(VAULT_FILE_NAME);
        let document = if path.exists() {
            match Self::load_document(&path, &local_key) {
                Ok(document) => document,
                Err(primary_error) => {
                    let backup = app_dir.join(BACKUP_FILE_NAME);
                    if !backup.exists() {
                        return Err(primary_error);
                    }
                    let document = Self::load_document(&backup, &local_key)?;
                    Self::write_document(&path, &document, &local_key)?;
                    document
                }
            }
        } else {
            let document = Self::new_document();
            Self::write_document(&path, &document, &local_key)?;
            document
        };
        Self::validate_document(&document)?;
        Ok(Self {
            path,
            local_key,
            document: Mutex::new(document),
        })
    }

    #[cfg(test)]
    pub(crate) fn open_for_test(app_dir: &Path) -> Result<Self, VaultError> {
        Self::open_with_key(app_dir, [7_u8; crate::local_security::LOCAL_KEY_LENGTH])
    }

    fn new_document() -> VaultDocument {
        let now = now();
        VaultDocument {
            format_version: FORMAT_VERSION,
            vault_id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            updated_at: now.clone(),
            profiles: Vec::new(),
            ssh_keys: Vec::new(),
            proxies: Vec::new(),
            ai_provider_config: None,
            ai_agents: vec![AiAgentConfig {
                id: DEFAULT_AGENT_ID.into(),
                name: DEFAULT_AGENT_NAME.into(),
                prompt: DEFAULT_AGENT_PROMPT.into(),
                is_default: true,
                created_at: now.clone(),
                updated_at: now,
            }],
            ai_executable_grants: Vec::new(),
            scripts: Vec::new(),
            terminal_settings: TerminalSettings::default(),
        }
    }

    fn load_document(
        path: &Path,
        key: &[u8; crate::local_security::LOCAL_KEY_LENGTH],
    ) -> Result<VaultDocument, VaultError> {
        let content = fs::read(path).map_err(io_error)?;
        let envelope: LocalEncryptedVault = serde_json::from_slice(&content).map_err(|_| {
            VaultError::InvalidFormat("expected an encrypted local Vault envelope".into())
        })?;
        let plaintext = decrypt_vault(&envelope, key)?;
        let document: VaultDocument = serde_json::from_slice(&plaintext)
            .map_err(|error| VaultError::InvalidFormat(error.to_string()))?;
        Self::validate_document(&document)?;
        Ok(document)
    }

    fn validate_document(document: &VaultDocument) -> Result<(), VaultError> {
        if document.format_version != FORMAT_VERSION {
            return Err(VaultError::InvalidFormat(format!(
                "unsupported formatVersion: {}",
                document.format_version
            )));
        }
        if uuid::Uuid::parse_str(&document.vault_id).is_err() {
            return Err(VaultError::InvalidFormat("vaultId must be a UUID".into()));
        }
        ensure_unique_uuids(
            document.profiles.iter().map(|profile| &profile.id),
            "profile",
        )?;
        ensure_unique_uuids(document.ssh_keys.iter().map(|key| &key.id), "SSH key")?;
        for key in &document.ssh_keys {
            Self::validate_key_request(&CreateKeyRequest {
                name: key.name.clone(),
                key_type: key.key_type.clone(),
                algorithm: (!key.algorithm.is_empty()).then(|| key.algorithm.clone()),
                private_key: key.private_key.clone(),
                cert_data: key.cert_data.clone(),
            })?;
        }
        ensure_unique_uuids(document.proxies.iter().map(|proxy| &proxy.id), "proxy")?;
        ensure_unique_ids(document.ai_agents.iter().map(|agent| &agent.id), "AI agent")?;
        let mut proxy_names = HashSet::new();
        for proxy in &document.proxies {
            validate_proxy_fields(
                &proxy.name,
                &proxy.host,
                proxy.port,
                &proxy.auth_type,
                proxy.username.as_deref(),
                proxy.password.as_deref(),
                true,
            )?;
            if !proxy_names.insert(proxy.name.as_str()) {
                return Err(VaultError::InvalidProxy(
                    "proxy names must be unique".into(),
                ));
            }
        }
        ensure_unique_uuids(document.scripts.iter().map(|script| &script.id), "script")?;
        let mut script_names = HashSet::new();
        for script in &document.scripts {
            validate_script_fields(
                &script.name,
                script.description.as_deref(),
                &script.tags,
                &script.command,
            )?;
            if !script_names.insert(script.name.as_str()) {
                return Err(VaultError::InvalidScript(
                    "script names must be unique".into(),
                ));
            }
        }

        for profile in &document.profiles {
            if let Some(proxy_id) = &profile.proxy_id {
                if !document.proxies.iter().any(|proxy| proxy.id == *proxy_id) {
                    return Err(VaultError::InvalidProxy(format!(
                        "profile {} references missing proxy {}",
                        profile.id, proxy_id
                    )));
                }
            }
            if let Some(key_id) = &profile.key_id {
                let key = document
                    .ssh_keys
                    .iter()
                    .find(|key| key.id == *key_id)
                    .ok_or_else(|| {
                        VaultError::InvalidSshKeyConfig(format!(
                            "profile {} references missing key {}",
                            profile.id, key_id
                        ))
                    })?;
                if profile.auth_type == AuthType::Certificate
                    && (key.key_type != "certificate" || key.cert_data.is_none())
                {
                    return Err(VaultError::InvalidSshKeyConfig(
                        "certificate authentication requires a key with an SSH user certificate"
                            .into(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn mutate<R>(
        &self,
        operation: impl FnOnce(&mut VaultDocument) -> Result<R, VaultError>,
    ) -> Result<R, VaultError> {
        let mut document = self
            .document
            .lock()
            .map_err(|_| VaultError::Storage("Vault lock poisoned".into()))?;
        let mut next = document.clone();
        let result = operation(&mut next)?;
        next.revision = next.revision.saturating_add(1);
        next.updated_at = now();
        Self::validate_document(&next)?;
        Self::write_document(&self.path, &next, &self.local_key)?;
        *document = next;
        Ok(result)
    }

    fn read<R>(
        &self,
        operation: impl FnOnce(&VaultDocument) -> Result<R, VaultError>,
    ) -> Result<R, VaultError> {
        let document = self
            .document
            .lock()
            .map_err(|_| VaultError::Storage("Vault lock poisoned".into()))?;
        operation(&document)
    }

    fn write_document(
        path: &Path,
        document: &VaultDocument,
        key: &[u8; crate::local_security::LOCAL_KEY_LENGTH],
    ) -> Result<(), VaultError> {
        let serialized =
            serde_json::to_vec(document).map_err(|error| VaultError::Storage(error.to_string()))?;
        let envelope = encrypt_vault(&serialized, key)?;
        let serialized = serde_json::to_vec_pretty(&envelope)
            .map_err(|error| VaultError::Storage(error.to_string()))?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| VaultError::Storage("invalid vault filename".into()))?;
        let temporary = path.with_file_name(format!(".{file_name}.tmp-{}", uuid::Uuid::new_v4()));

        let result = (|| -> Result<(), VaultError> {
            let mut file = File::create(&temporary).map_err(io_error)?;
            file.write_all(&serialized).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            drop(file);

            if path.exists() {
                fs::copy(path, path.with_file_name(BACKUP_FILE_NAME)).map_err(io_error)?;
            }
            fs::rename(&temporary, path).map_err(io_error)
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn sync_snapshot(&self) -> Result<VaultSyncSnapshot, VaultError> {
        self.read(|document| {
            let content = serde_json::to_vec(document)
                .map_err(|error| VaultError::Storage(error.to_string()))?;
            Ok(VaultSyncSnapshot {
                content,
                vault_id: document.vault_id.clone(),
                revision: document.revision,
            })
        })
    }

    pub fn replace_from_sync(&self, content: &[u8]) -> Result<(), VaultError> {
        let document: VaultDocument = serde_json::from_slice(content)
            .map_err(|error| VaultError::InvalidFormat(error.to_string()))?;
        Self::validate_document(&document)?;
        Self::write_document(&self.path, &document, &self.local_key)?;
        let mut current = self
            .document
            .lock()
            .map_err(|_| VaultError::Storage("Vault lock poisoned".into()))?;
        *current = document;
        Ok(())
    }

    pub fn list_scripts(&self) -> Result<Vec<Script>, VaultError> {
        self.read(|document| {
            let mut scripts = document.scripts.clone();
            scripts.sort_by_key(|script| script.name.to_lowercase());
            Ok(scripts)
        })
    }

    pub fn get_script(&self, id: &str) -> Result<Script, VaultError> {
        self.read(|document| {
            document
                .scripts
                .iter()
                .find(|script| script.id == id)
                .cloned()
                .ok_or_else(|| VaultError::ScriptNotFound(id.into()))
        })
    }

    pub fn create_script(&self, request: &CreateScriptRequest) -> Result<Script, VaultError> {
        validate_script_fields(
            &request.name,
            request.description.as_deref(),
            &request.tags,
            &request.command,
        )?;
        self.mutate(|document| {
            let name = request.name.trim();
            if document.scripts.iter().any(|script| script.name == name) {
                return Err(VaultError::InvalidScript(
                    "script name already exists".into(),
                ));
            }
            let timestamp = now();
            let script = Script {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.into(),
                description: normalized_optional_text(request.description.as_deref()),
                tags: normalized_tags(&request.tags),
                command: request.command.clone(),
                risk_level: request.risk_level.clone(),
                created_at: timestamp.clone(),
                updated_at: timestamp,
            };
            document.scripts.push(script.clone());
            Ok(script)
        })
    }

    pub fn update_script(
        &self,
        id: &str,
        request: &UpdateScriptRequest,
    ) -> Result<Script, VaultError> {
        self.mutate(|document| {
            let index = document
                .scripts
                .iter()
                .position(|script| script.id == id)
                .ok_or_else(|| VaultError::ScriptNotFound(id.into()))?;
            let existing = document.scripts[index].clone();
            let name = request.name.as_deref().unwrap_or(&existing.name);
            let description = request
                .description
                .as_deref()
                .or(existing.description.as_deref());
            let tags = request.tags.as_deref().unwrap_or(&existing.tags);
            let command = request.command.as_deref().unwrap_or(&existing.command);
            validate_script_fields(name, description, tags, command)?;
            let normalized_name = name.trim();
            if document
                .scripts
                .iter()
                .any(|script| script.id != id && script.name == normalized_name)
            {
                return Err(VaultError::InvalidScript(
                    "script name already exists".into(),
                ));
            }
            let script = &mut document.scripts[index];
            script.name = normalized_name.into();
            script.description = request
                .description
                .as_deref()
                .map(|value| normalized_optional_text(Some(value)))
                .unwrap_or(existing.description);
            script.tags = normalized_tags(tags);
            script.command = command.into();
            script.risk_level = request.risk_level.clone().unwrap_or(existing.risk_level);
            script.updated_at = now();
            Ok(script.clone())
        })
    }

    pub fn delete_script(&self, id: &str) -> Result<(), VaultError> {
        self.mutate(|document| {
            let before = document.scripts.len();
            document.scripts.retain(|script| script.id != id);
            if document.scripts.len() == before {
                return Err(VaultError::ScriptNotFound(id.into()));
            }
            Ok(())
        })
    }

    pub fn terminal_settings(&self) -> Result<TerminalSettings, VaultError> {
        self.read(|document| Ok(document.terminal_settings.clone()))
    }

    pub fn save_terminal_settings(
        &self,
        request: &SaveTerminalSettingsRequest,
    ) -> Result<TerminalSettings, VaultError> {
        let terminal_type = request.terminal_type.trim();
        if !matches!(terminal_type, "xterm-256color" | "xterm" | "vt100") {
            return Err(VaultError::InvalidTerminalSettings(
                "unsupported terminal type".into(),
            ));
        }
        if !(8..=24).contains(&request.font_size) {
            return Err(VaultError::InvalidTerminalSettings(
                "font_size must be between 8 and 24".into(),
            ));
        }
        let font_family = request.font_family.trim();
        if font_family.is_empty() || font_family.len() > 500 {
            return Err(VaultError::InvalidTerminalSettings(
                "font_family must contain between 1 and 500 characters".into(),
            ));
        }
        if !(1_000..=50_000).contains(&request.scrollback_lines) {
            return Err(VaultError::InvalidTerminalSettings(
                "scrollback_lines must be between 1000 and 50000".into(),
            ));
        }
        if !matches!(request.backspace_sends.as_str(), "del" | "bs") {
            return Err(VaultError::InvalidTerminalSettings(
                "backspace_sends must be del or bs".into(),
            ));
        }
        if !(5..=120).contains(&request.connect_timeout_seconds) {
            return Err(VaultError::InvalidTerminalSettings(
                "connect_timeout_seconds must be between 5 and 120".into(),
            ));
        }
        if request.keepalive_interval_seconds != 0
            && !(15..=300).contains(&request.keepalive_interval_seconds)
        {
            return Err(VaultError::InvalidTerminalSettings(
                "keepalive_interval_seconds must be 0 or between 15 and 300".into(),
            ));
        }

        let settings = TerminalSettings {
            terminal_type: terminal_type.into(),
            font_size: request.font_size,
            font_family: font_family.into(),
            scrollback_lines: request.scrollback_lines,
            backspace_sends: request.backspace_sends.clone(),
            alt_sends_escape: request.alt_sends_escape,
            connect_timeout_seconds: request.connect_timeout_seconds,
            keepalive_interval_seconds: request.keepalive_interval_seconds,
        };
        self.mutate(|document| {
            document.terminal_settings = settings.clone();
            Ok(settings)
        })
    }

    pub fn list_profiles(&self) -> Result<Vec<SshProfile>, VaultError> {
        self.read(|document| Ok(document.profiles.clone()))
    }

    pub fn get_profile(&self, id: &str) -> Result<SshProfile, VaultError> {
        self.read(|document| {
            document
                .profiles
                .iter()
                .find(|profile| profile.id == id)
                .cloned()
                .ok_or_else(|| VaultError::ProfileNotFound(id.into()))
        })
    }

    pub fn create_profile(&self, request: &CreateProfileRequest) -> Result<SshProfile, VaultError> {
        self.mutate(|document| {
            Self::validate_certificate_key(
                document,
                &request.auth_type,
                request.key_id.as_deref(),
            )?;
            if let Some(proxy_id) = &request.proxy_id {
                if !document.proxies.iter().any(|proxy| proxy.id == *proxy_id) {
                    return Err(VaultError::InvalidProxy(format!(
                        "Proxy not found: {proxy_id}"
                    )));
                }
            }
            let now = now();
            let profile = SshProfile {
                id: uuid::Uuid::new_v4().to_string(),
                name: request.name.clone(),
                host: request.host.clone(),
                port: request.port.unwrap_or(22),
                username: request.username.clone(),
                auth_type: request.auth_type.clone(),
                credential: request
                    .credential
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
                key_id: request.key_id.clone(),
                proxy_id: request.proxy_id.clone(),
                group_name: request.group_name.clone(),
                icon: request.icon.clone(),
                color: request.color.clone(),
                os: request.os.clone(),
                location: request.location.clone(),
                created_at: now.clone(),
                updated_at: now,
            };
            document.profiles.push(profile.clone());
            Ok(profile)
        })
    }

    pub fn update_profile(
        &self,
        id: &str,
        request: &UpdateProfileRequest,
    ) -> Result<SshProfile, VaultError> {
        self.mutate(|document| {
            let index = document
                .profiles
                .iter()
                .position(|profile| profile.id == id)
                .ok_or_else(|| VaultError::ProfileNotFound(id.into()))?;
            let existing = document.profiles[index].clone();
            let auth_type = request
                .auth_type
                .clone()
                .unwrap_or(existing.auth_type.clone());
            let key_id = request.key_id.clone().or(existing.key_id.clone());
            let proxy_id = if request.clear_proxy {
                None
            } else {
                request.proxy_id.clone().or(existing.proxy_id.clone())
            };
            Self::validate_certificate_key(document, &auth_type, key_id.as_deref())?;
            if let Some(proxy_id) = &proxy_id {
                if !document.proxies.iter().any(|proxy| proxy.id == *proxy_id) {
                    return Err(VaultError::InvalidProxy(format!(
                        "Proxy not found: {proxy_id}"
                    )));
                }
            }
            let profile = &mut document.profiles[index];
            profile.name = request.name.clone().unwrap_or(existing.name);
            profile.host = request.host.clone().unwrap_or(existing.host);
            profile.port = request.port.unwrap_or(existing.port);
            profile.username = request.username.clone().unwrap_or(existing.username);
            profile.auth_type = auth_type;
            profile.credential = match &request.credential {
                Some(value) if value.is_empty() => None,
                Some(value) => Some(value.clone()),
                None => existing.credential,
            };
            profile.key_id = key_id;
            profile.proxy_id = proxy_id;
            profile.group_name = request.group_name.clone().or(existing.group_name);
            profile.icon = request.icon.clone().or(existing.icon);
            profile.color = request.color.clone().or(existing.color);
            profile.os = request.os.clone().or(existing.os);
            profile.location = request.location.clone().or(existing.location);
            profile.updated_at = now();
            Ok(profile.clone())
        })
    }

    pub fn delete_profile(&self, id: &str) -> Result<(), VaultError> {
        self.mutate(|document| {
            let before = document.profiles.len();
            document.profiles.retain(|profile| profile.id != id);
            if document.profiles.len() == before {
                return Err(VaultError::ProfileNotFound(id.into()));
            }
            Ok(())
        })
    }

    pub fn list_proxies(&self) -> Result<Vec<Socks5ProxyView>, VaultError> {
        self.read(|document| {
            let mut proxies: Vec<_> = document.proxies.iter().map(proxy_view).collect();
            proxies.sort_by_key(|proxy| proxy.name.to_lowercase());
            Ok(proxies)
        })
    }

    pub fn get_proxy(&self, id: &str) -> Result<Socks5Proxy, VaultError> {
        self.read(|document| {
            document
                .proxies
                .iter()
                .find(|proxy| proxy.id == id)
                .cloned()
                .ok_or_else(|| VaultError::ProxyNotFound(id.into()))
        })
    }

    pub fn create_proxy(
        &self,
        request: &CreateSocks5ProxyRequest,
    ) -> Result<Socks5ProxyView, VaultError> {
        validate_proxy_fields(
            &request.name,
            &request.host,
            request.port,
            &request.auth_type,
            request.username.as_deref(),
            request.password.as_deref(),
            true,
        )?;
        self.mutate(|document| {
            let name = request.name.trim();
            if document.proxies.iter().any(|proxy| proxy.name == name) {
                return Err(VaultError::InvalidProxy("proxy name already exists".into()));
            }
            let timestamp = now();
            let (username, password) = normalized_proxy_credentials(request);
            let proxy = Socks5Proxy {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.into(),
                host: request.host.trim().into(),
                port: request.port,
                auth_type: request.auth_type.clone(),
                username,
                password,
                created_at: timestamp.clone(),
                updated_at: timestamp,
            };
            let view = proxy_view(&proxy);
            document.proxies.push(proxy);
            Ok(view)
        })
    }

    pub fn update_proxy(
        &self,
        id: &str,
        request: &UpdateSocks5ProxyRequest,
    ) -> Result<Socks5ProxyView, VaultError> {
        self.mutate(|document| {
            let index = document
                .proxies
                .iter()
                .position(|proxy| proxy.id == id)
                .ok_or_else(|| VaultError::ProxyNotFound(id.into()))?;
            let existing = document.proxies[index].clone();
            let password = if request.auth_type == Socks5ProxyAuthType::Password {
                request
                    .password
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .or(existing.password.as_deref())
            } else {
                None
            };
            validate_proxy_fields(
                &request.name,
                &request.host,
                request.port,
                &request.auth_type,
                request.username.as_deref(),
                password,
                true,
            )?;
            let name = request.name.trim();
            if document
                .proxies
                .iter()
                .any(|proxy| proxy.id != id && proxy.name == name)
            {
                return Err(VaultError::InvalidProxy("proxy name already exists".into()));
            }
            let proxy = &mut document.proxies[index];
            proxy.name = name.into();
            proxy.host = request.host.trim().into();
            proxy.port = request.port;
            proxy.auth_type = request.auth_type.clone();
            if request.auth_type == Socks5ProxyAuthType::Password {
                proxy.username = normalized_optional_text(request.username.as_deref());
                proxy.password = password.map(str::to_owned);
            } else {
                proxy.username = None;
                proxy.password = None;
            }
            proxy.updated_at = now();
            Ok(proxy_view(proxy))
        })
    }

    pub fn delete_proxy(&self, id: &str) -> Result<(), VaultError> {
        self.mutate(|document| {
            let before = document.proxies.len();
            document.proxies.retain(|proxy| proxy.id != id);
            if document.proxies.len() == before {
                return Err(VaultError::ProxyNotFound(id.into()));
            }
            for profile in &mut document.profiles {
                if profile.proxy_id.as_deref() == Some(id) {
                    profile.proxy_id = None;
                    profile.updated_at = now();
                }
            }
            Ok(())
        })
    }

    pub fn decrypt_credential(
        &self,
        profile: &SshProfile,
    ) -> Result<DecryptedCredential, VaultError> {
        self.read(|document| {
            let key = profile
                .key_id
                .as_deref()
                .map(|key_id| {
                    document
                        .ssh_keys
                        .iter()
                        .find(|key| key.id == key_id)
                        .cloned()
                        .ok_or_else(|| {
                            VaultError::InvalidSshKeyConfig(format!("key not found: {key_id}"))
                        })
                })
                .transpose()?;
            Ok(DecryptedCredential {
                password: (profile.auth_type == AuthType::Password)
                    .then(|| profile.credential.clone())
                    .flatten(),
                private_key: key.as_ref().map(|key| key.private_key.clone()),
                key_algorithm: key.as_ref().map(|key| key.algorithm.clone()),
                cert_data: key.as_ref().and_then(|key| key.cert_data.clone()),
            })
        })
    }

    pub fn list_keys(&self) -> Result<Vec<SshKeyView>, VaultError> {
        self.read(|document| {
            let mut keys: Vec<_> = document
                .ssh_keys
                .iter()
                .map(|key| SshKeyView {
                    id: key.id.clone(),
                    name: key.name.clone(),
                    key_type: key.key_type.clone(),
                    algorithm: key.algorithm.clone(),
                    created_at: key.created_at.clone(),
                    updated_at: key.updated_at.clone(),
                })
                .collect();
            keys.sort_by_key(|key| key.name.to_lowercase());
            Ok(keys)
        })
    }

    pub fn get_key(&self, id: &str) -> Result<SshKey, VaultError> {
        self.read(|document| {
            document
                .ssh_keys
                .iter()
                .find(|key| key.id == id)
                .cloned()
                .ok_or_else(|| VaultError::InvalidSshKeyConfig(format!("Key not found: {id}")))
        })
    }

    pub fn create_key(&self, request: &CreateKeyRequest) -> Result<SshKeyView, VaultError> {
        let algorithm = Self::validate_key_request(request)?;
        self.mutate(|document| {
            let now = now();
            let key = SshKey {
                id: uuid::Uuid::new_v4().to_string(),
                name: request.name.clone(),
                key_type: request.key_type.clone(),
                algorithm: algorithm.clone(),
                private_key: request.private_key.clone(),
                cert_data: request
                    .cert_data
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
                created_at: now.clone(),
                updated_at: now,
            };
            let view = key_view(&key);
            document.ssh_keys.push(key);
            Ok(view)
        })
    }

    pub fn update_key(
        &self,
        id: &str,
        request: &CreateKeyRequest,
    ) -> Result<SshKeyView, VaultError> {
        let algorithm = Self::validate_key_request(request)?;
        self.mutate(|document| {
            let key = document
                .ssh_keys
                .iter_mut()
                .find(|key| key.id == id)
                .ok_or_else(|| VaultError::InvalidSshKeyConfig(format!("Key not found: {id}")))?;
            key.name = request.name.clone();
            key.key_type = request.key_type.clone();
            key.algorithm = algorithm.clone();
            key.private_key = request.private_key.clone();
            key.cert_data = request
                .cert_data
                .as_deref()
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            key.updated_at = now();
            Ok(key_view(key))
        })
    }

    pub fn delete_key(&self, id: &str) -> Result<(), VaultError> {
        self.mutate(|document| {
            let before = document.ssh_keys.len();
            document.ssh_keys.retain(|key| key.id != id);
            if document.ssh_keys.len() == before {
                return Err(VaultError::InvalidSshKeyConfig(format!(
                    "Key not found: {id}"
                )));
            }
            for profile in &mut document.profiles {
                if profile.key_id.as_deref() == Some(id) {
                    profile.key_id = None;
                    profile.updated_at = now();
                }
            }
            Ok(())
        })
    }

    pub fn get_ai_config_view(&self) -> Result<AiProviderConfigView, VaultError> {
        self.read(|document| match &document.ai_provider_config {
            Some(config) => {
                let model = Self::active_ai_model(&config.models, &config.active_model_id)?.clone();
                Ok(AiProviderConfigView {
                    configured: true,
                    provider_type: config.provider_type.clone(),
                    base_url: Some(config.base_url.clone()),
                    model: Some(model.name),
                    timeout_seconds: Some(config.timeout_seconds),
                    models: config.models.clone(),
                    active_model_id: Some(config.active_model_id.clone()),
                })
            }
            None => Ok(AiProviderConfigView {
                configured: false,
                provider_type: "openai_compatible".into(),
                base_url: None,
                model: None,
                timeout_seconds: None,
                models: Vec::new(),
                active_model_id: None,
            }),
        })
    }

    pub fn save_ai_config(&self, request: &SaveAiProviderConfigRequest) -> Result<(), VaultError> {
        let base_url = request.base_url.trim().trim_end_matches('/');
        if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
            return Err(VaultError::InvalidAiConfig(
                "base_url must start with http:// or https://".into(),
            ));
        }
        if !(10..=300).contains(&request.timeout_seconds) {
            return Err(VaultError::InvalidAiConfig(
                "timeout_seconds must be between 10 and 300".into(),
            ));
        }
        let models = Self::normalized_ai_models(request)?;
        let active_model_id = request
            .active_model_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .or_else(|| {
                models
                    .iter()
                    .find(|model| model.name == request.model.trim())
                    .map(|model| model.id.as_str())
            })
            .unwrap_or("default")
            .to_owned();
        Self::active_ai_model(&models, &active_model_id)?;

        self.mutate(|document| {
            let api_key = if request.api_key.trim().is_empty() {
                document
                    .ai_provider_config
                    .as_ref()
                    .map(|config| config.api_key.clone())
                    .ok_or_else(|| {
                        VaultError::InvalidAiConfig(
                            "api_key is required when creating a configuration".into(),
                        )
                    })?
            } else {
                request.api_key.trim().to_owned()
            };
            let timestamp = now();
            let created_at = document
                .ai_provider_config
                .as_ref()
                .map(|config| config.created_at.clone())
                .unwrap_or_else(|| timestamp.clone());
            document.ai_provider_config = Some(StoredAiProviderConfig {
                provider_type: "openai_compatible".into(),
                base_url: base_url.into(),
                api_key,
                active_model_id: active_model_id.clone(),
                models: models.clone(),
                timeout_seconds: request.timeout_seconds,
                created_at,
                updated_at: timestamp,
            });
            Ok(())
        })
    }

    pub fn get_ai_config_secret_for_model(
        &self,
        model_id: Option<&str>,
    ) -> Result<Option<AiProviderConfigSecret>, VaultError> {
        self.read(|document| {
            let Some(config) = &document.ai_provider_config else {
                return Ok(None);
            };
            let model_id = model_id
                .filter(|id| !id.trim().is_empty())
                .unwrap_or(&config.active_model_id);
            let model = Self::active_ai_model(&config.models, model_id)?.clone();
            Ok(Some(AiProviderConfigSecret {
                base_url: config.base_url.clone(),
                api_key: config.api_key.clone(),
                model,
                timeout_seconds: config.timeout_seconds,
            }))
        })
    }

    pub fn get_ai_config_secret(&self) -> Result<Option<AiProviderConfigSecret>, VaultError> {
        self.get_ai_config_secret_for_model(None)
    }

    pub fn delete_ai_config(&self) -> Result<(), VaultError> {
        self.mutate(|document| {
            document.ai_provider_config = None;
            Ok(())
        })
    }

    pub fn has_ai_executable_grant(
        &self,
        executable: &str,
        server_key: &str,
    ) -> Result<bool, VaultError> {
        self.read(|document| {
            Ok(document.ai_executable_grants.iter().any(|grant| {
                grant.executable_name == executable
                    && ((grant.scope == "global" && grant.scope_target.is_empty())
                        || (grant.scope == "server" && grant.scope_target == server_key))
            }))
        })
    }

    pub fn save_ai_executable_grant(
        &self,
        executable: &str,
        scope: &str,
        scope_target: &str,
    ) -> Result<(), VaultError> {
        if scope != "global" && scope != "server" {
            return Err(VaultError::InvalidAiConfig(
                "grant scope must be global or server".into(),
            ));
        }
        self.mutate(|document| {
            let timestamp = now();
            if let Some(grant) = document.ai_executable_grants.iter_mut().find(|grant| {
                grant.executable_name == executable
                    && grant.scope == scope
                    && grant.scope_target == scope_target
            }) {
                grant.updated_at = timestamp;
                return Ok(());
            }
            document.ai_executable_grants.push(StoredAiExecutableGrant {
                executable_name: executable.into(),
                scope: scope.into(),
                scope_target: scope_target.into(),
                created_at: timestamp.clone(),
                updated_at: timestamp,
            });
            Ok(())
        })
    }

    pub fn list_ai_agents(&self) -> Result<Vec<AiAgentConfig>, VaultError> {
        self.read(|document| {
            let mut agents = document.ai_agents.clone();
            agents.sort_by(|left, right| {
                right
                    .is_default
                    .cmp(&left.is_default)
                    .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            });
            Ok(agents)
        })
    }

    pub fn get_ai_agent(&self, id: &str) -> Result<AiAgentConfig, VaultError> {
        self.read(|document| {
            document
                .ai_agents
                .iter()
                .find(|agent| agent.id == id)
                .cloned()
                .ok_or_else(|| VaultError::AiAgentNotFound(id.into()))
        })
    }

    pub fn get_default_ai_agent(&self) -> Result<AiAgentConfig, VaultError> {
        self.read(|document| {
            document
                .ai_agents
                .iter()
                .find(|agent| agent.is_default)
                .cloned()
                .ok_or_else(|| VaultError::AiAgentNotFound("default".into()))
        })
    }

    pub fn save_ai_agent(
        &self,
        request: &SaveAiAgentConfigRequest,
    ) -> Result<AiAgentConfig, VaultError> {
        let name = request.name.trim();
        let prompt = request.prompt.trim();
        if name.is_empty() || prompt.is_empty() {
            return Err(VaultError::InvalidAiConfig(
                "agent name and prompt are required".into(),
            ));
        }
        if name.len() > 80 || prompt.len() > 16 * 1024 {
            return Err(VaultError::InvalidAiConfig(
                "agent name or prompt is too long".into(),
            ));
        }
        self.mutate(|document| {
            let timestamp = now();
            let id = request
                .id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            if let Some(agent) = document.ai_agents.iter_mut().find(|agent| agent.id == id) {
                agent.name = name.into();
                agent.prompt = prompt.into();
                agent.updated_at = timestamp;
                return Ok(agent.clone());
            }
            let agent = AiAgentConfig {
                id,
                name: name.into(),
                prompt: prompt.into(),
                is_default: false,
                created_at: timestamp.clone(),
                updated_at: timestamp,
            };
            document.ai_agents.push(agent.clone());
            Ok(agent)
        })
    }

    pub fn delete_ai_agent(&self, id: &str) -> Result<(), VaultError> {
        self.mutate(|document| {
            let agent = document
                .ai_agents
                .iter()
                .find(|agent| agent.id == id)
                .ok_or_else(|| VaultError::AiAgentNotFound(id.into()))?;
            if agent.is_default {
                return Err(VaultError::DefaultAiAgentCannotBeDeleted);
            }
            document.ai_agents.retain(|agent| agent.id != id);
            Ok(())
        })
    }

    fn validate_certificate_key(
        document: &VaultDocument,
        auth_type: &AuthType,
        key_id: Option<&str>,
    ) -> Result<(), VaultError> {
        if *auth_type != AuthType::Certificate {
            return Ok(());
        }
        let key_id = key_id.ok_or_else(|| {
            VaultError::InvalidSshKeyConfig("certificate authentication requires a key".into())
        })?;
        let key = document
            .ssh_keys
            .iter()
            .find(|key| key.id == key_id)
            .ok_or_else(|| VaultError::InvalidSshKeyConfig(format!("Key not found: {key_id}")))?;
        if key.key_type != "certificate" || key.cert_data.as_deref().map_or(true, str::is_empty) {
            return Err(VaultError::InvalidSshKeyConfig(
                "certificate authentication requires a key with an SSH user certificate".into(),
            ));
        }
        Ok(())
    }

    fn validate_key_request(request: &CreateKeyRequest) -> Result<String, VaultError> {
        let private_key =
            PrivateKey::from_openssh(request.private_key.trim()).map_err(|error| {
                VaultError::InvalidSshKeyConfig(format!("invalid OpenSSH private key: {error}"))
            })?;
        let algorithm = private_key.algorithm().as_str().to_owned();

        if let Some(selected_algorithm) = request
            .algorithm
            .as_deref()
            .map(str::trim)
            .filter(|algorithm| !algorithm.is_empty() && *algorithm != "auto")
        {
            if selected_algorithm != algorithm {
                return Err(VaultError::InvalidSshKeyConfig(format!(
                    "selected algorithm {selected_algorithm} does not match private key algorithm {algorithm}"
                )));
            }
        }

        if request.key_type == "certificate" {
            let certificate_data = request
                .cert_data
                .as_deref()
                .map(str::trim)
                .filter(|certificate| !certificate.is_empty())
                .ok_or_else(|| {
                    VaultError::InvalidSshKeyConfig(
                        "an SSH user certificate is required for certificate keys".into(),
                    )
                })?;
            let certificate = Certificate::from_openssh(certificate_data).map_err(|error| {
                VaultError::InvalidSshKeyConfig(format!(
                    "invalid OpenSSH user certificate: {error}"
                ))
            })?;

            if !certificate.cert_type().is_user() {
                return Err(VaultError::InvalidSshKeyConfig(
                    "certificate keys require an SSH user certificate, not a host certificate"
                        .into(),
                ));
            }
            if certificate.algorithm() != private_key.algorithm()
                || certificate.public_key() != private_key.public_key().key_data()
            {
                return Err(VaultError::InvalidSshKeyConfig(
                    "certificate public key does not match the private key".into(),
                ));
            }
        }

        Ok(algorithm)
    }

    fn normalized_ai_models(
        request: &SaveAiProviderConfigRequest,
    ) -> Result<Vec<AiModelConfig>, VaultError> {
        let mut models = if request.models.is_empty() {
            let name = request.model.trim();
            if name.is_empty() {
                return Err(VaultError::InvalidAiConfig(
                    "model is required when models is omitted".into(),
                ));
            }
            vec![AiModelConfig {
                id: "default".into(),
                name: name.into(),
                max_context_tokens: None,
                max_output_tokens: None,
                supports_tools: false,
                supports_images: false,
                supports_parallel_tool_calls: false,
                supports_prompt_caching: false,
                supports_reasoning: false,
                protocol: "chat_completions".into(),
                reasoning_effort: None,
                prompt_cache_key: None,
            }]
        } else {
            request.models.clone()
        };
        if !(1..=20).contains(&models.len()) {
            return Err(VaultError::InvalidAiConfig(
                "models must contain between 1 and 20 entries".into(),
            ));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for model in &mut models {
            model.id = model.id.trim().into();
            model.name = model.name.trim().into();
            model.protocol = model.protocol.trim().into();
            model.reasoning_effort = model
                .reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            model.prompt_cache_key = model
                .prompt_cache_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            if model.id.is_empty()
                || model.name.is_empty()
                || !ids.insert(model.id.clone())
                || !names.insert(model.name.clone())
            {
                return Err(VaultError::InvalidAiConfig(
                    "each model requires a unique non-empty id and name".into(),
                ));
            }
            if model.protocol != "chat_completions" && model.protocol != "responses" {
                return Err(VaultError::InvalidAiConfig(
                    "model protocol must be chat_completions or responses".into(),
                ));
            }
        }
        Ok(models)
    }

    fn active_ai_model<'a>(
        models: &'a [AiModelConfig],
        id: &str,
    ) -> Result<&'a AiModelConfig, VaultError> {
        models
            .iter()
            .find(|model| model.id == id)
            .ok_or_else(|| VaultError::InvalidAiConfig(format!("unknown model id: {id}")))
    }
}

fn validate_proxy_fields(
    name: &str,
    host: &str,
    port: u16,
    auth_type: &Socks5ProxyAuthType,
    username: Option<&str>,
    password: Option<&str>,
    require_password: bool,
) -> Result<(), VaultError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(VaultError::InvalidProxy(
            "name must contain 1 to 80 characters".into(),
        ));
    }
    let host = host.trim();
    if host.is_empty() || host.len() > 255 {
        return Err(VaultError::InvalidProxy(
            "host must contain 1 to 255 characters".into(),
        ));
    }
    if port == 0 {
        return Err(VaultError::InvalidProxy(
            "port must be between 1 and 65535".into(),
        ));
    }
    if *auth_type == Socks5ProxyAuthType::Password {
        if username
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            return Err(VaultError::InvalidProxy(
                "username is required for password authentication".into(),
            ));
        }
        if require_password && password.filter(|value| !value.is_empty()).is_none() {
            return Err(VaultError::InvalidProxy(
                "password is required for password authentication".into(),
            ));
        }
        if username.is_some_and(|value| value.len() > 255)
            || password.is_some_and(|value| value.len() > 255)
        {
            return Err(VaultError::InvalidProxy(
                "username and password must be at most 255 bytes".into(),
            ));
        }
    }
    Ok(())
}

fn normalized_proxy_credentials(
    request: &CreateSocks5ProxyRequest,
) -> (Option<String>, Option<String>) {
    if request.auth_type == Socks5ProxyAuthType::Password {
        (
            normalized_optional_text(request.username.as_deref()),
            normalized_optional_text(request.password.as_deref()),
        )
    } else {
        (None, None)
    }
}

fn proxy_view(proxy: &Socks5Proxy) -> Socks5ProxyView {
    Socks5ProxyView {
        id: proxy.id.clone(),
        name: proxy.name.clone(),
        host: proxy.host.clone(),
        port: proxy.port,
        auth_type: proxy.auth_type.clone(),
        username: proxy.username.clone(),
        created_at: proxy.created_at.clone(),
        updated_at: proxy.updated_at.clone(),
    }
}

fn validate_script_fields(
    name: &str,
    description: Option<&str>,
    tags: &[String],
    command: &str,
) -> Result<(), VaultError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(VaultError::InvalidScript(
            "name must contain 1 to 80 characters".into(),
        ));
    }
    if description.is_some_and(|value| value.chars().count() > 500) {
        return Err(VaultError::InvalidScript(
            "description must be at most 500 characters".into(),
        ));
    }
    if tags.len() > 10
        || tags
            .iter()
            .any(|tag| tag.trim().is_empty() || tag.chars().count() > 32)
    {
        return Err(VaultError::InvalidScript(
            "scripts allow at most 10 non-empty tags of 32 characters".into(),
        ));
    }
    if command.trim().is_empty() || command.len() > 32 * 1024 {
        return Err(VaultError::InvalidScript(
            "command must be non-empty and at most 32 KiB".into(),
        ));
    }
    Ok(())
}

fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn normalized_tags(tags: &[String]) -> Vec<String> {
    tags.iter().map(|tag| tag.trim().to_owned()).collect()
}

fn ensure_unique_uuids<'a>(
    values: impl Iterator<Item = &'a String>,
    entity: &str,
) -> Result<(), VaultError> {
    let mut seen = HashSet::new();
    for value in values {
        if uuid::Uuid::parse_str(value).is_err() || !seen.insert(value) {
            return Err(VaultError::InvalidFormat(format!(
                "{entity} IDs must be unique UUIDs"
            )));
        }
    }
    Ok(())
}

fn ensure_unique_ids<'a>(
    values: impl Iterator<Item = &'a String>,
    entity: &str,
) -> Result<(), VaultError> {
    let mut seen = HashSet::new();
    for value in values {
        if value.trim().is_empty() || !seen.insert(value) {
            return Err(VaultError::InvalidFormat(format!(
                "{entity} IDs must be unique and non-empty"
            )));
        }
    }
    Ok(())
}

fn key_view(key: &SshKey) -> SshKeyView {
    SshKeyView {
        id: key.id.clone(),
        name: key.name.clone(),
        key_type: key.key_type.clone(),
        algorithm: key.algorithm.clone(),
        created_at: key.created_at.clone(),
        updated_at: key.updated_at.clone(),
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn io_error(error: std::io::Error) -> VaultError {
    VaultError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_private_key() -> String {
        let mut seed =
            <rand_chacha::ChaCha20Rng as rand_chacha::rand_core::SeedableRng>::Seed::default();
        getrandom::fill(&mut seed).unwrap();
        let mut rng =
            <rand_chacha::ChaCha20Rng as rand_chacha::rand_core::SeedableRng>::from_seed(seed);
        PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519)
            .unwrap()
            .to_openssh(ssh_key::LineEnding::LF)
            .unwrap()
            .to_string()
    }

    fn test_dir() -> PathBuf {
        std::env::temp_dir().join(format!("mjj-ssh-vault-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn creates_persists_and_recovers_encrypted_vault() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();
        let private_key = test_private_key();
        let key = vault
            .create_key(&CreateKeyRequest {
                name: "Production".into(),
                key_type: "key".into(),
                algorithm: None,
                private_key: private_key.clone(),
                cert_data: None,
            })
            .unwrap();
        let profile = vault
            .create_profile(&CreateProfileRequest {
                name: "Production".into(),
                host: "example.test".into(),
                port: Some(22),
                username: "root".into(),
                auth_type: AuthType::Key,
                credential: None,
                key_id: Some(key.id.clone()),
                proxy_id: None,
                group_name: None,
                icon: None,
                color: None,
                os: None,
                location: None,
            })
            .unwrap();
        let credential = vault.decrypt_credential(&profile).unwrap();
        assert_eq!(
            credential.private_key.as_deref(),
            Some(private_key.as_str())
        );
        assert_eq!(credential.key_algorithm.as_deref(), Some("ssh-ed25519"));
        drop(vault);

        let persisted = fs::read_to_string(directory.join(VAULT_FILE_NAME)).unwrap();
        assert!(!persisted.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(persisted.contains("\"ciphertext\""));
        let reopened = Vault::open_for_test(&directory).unwrap();
        assert_eq!(reopened.list_profiles().unwrap().len(), 1);
        assert!(directory.join(VAULT_FILE_NAME).exists());
        assert!(directory.join(BACKUP_FILE_NAME).exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn persists_terminal_settings_and_rejects_invalid_values() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();
        let request = SaveTerminalSettingsRequest {
            terminal_type: "xterm".into(),
            font_size: 16,
            font_family: "Cascadia Code, Consolas, monospace".into(),
            scrollback_lines: 10_000,
            backspace_sends: "bs".into(),
            alt_sends_escape: false,
            connect_timeout_seconds: 45,
            keepalive_interval_seconds: 120,
        };
        let settings = vault.save_terminal_settings(&request).unwrap();
        assert_eq!(settings.terminal_type, "xterm");
        assert_eq!(settings.keepalive_interval_seconds, 120);
        assert!(matches!(
            vault.save_terminal_settings(&SaveTerminalSettingsRequest {
                keepalive_interval_seconds: 1,
                ..request.clone()
            }),
            Err(VaultError::InvalidTerminalSettings(_))
        ));
        drop(vault);

        let reopened = Vault::open_for_test(&directory).unwrap();
        assert_eq!(reopened.terminal_settings().unwrap().font_size, 16);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_invalid_private_key_imports() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();

        let error = vault
            .create_key(&CreateKeyRequest {
                name: "Invalid".into(),
                key_type: "key".into(),
                algorithm: None,
                private_key: "not-a-private-key".into(),
                cert_data: None,
            })
            .unwrap_err();

        assert!(matches!(error, VaultError::InvalidSshKeyConfig(_)));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_import_algorithm_that_does_not_match_private_key() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();

        let error = vault
            .create_key(&CreateKeyRequest {
                name: "Mismatched algorithm".into(),
                key_type: "key".into(),
                algorithm: Some("ssh-rsa".into()),
                private_key: test_private_key(),
                cert_data: None,
            })
            .unwrap_err();

        assert!(matches!(error, VaultError::InvalidSshKeyConfig(_)));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_invalid_certificate_content() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();

        let error = vault
            .create_key(&CreateKeyRequest {
                name: "Invalid certificate".into(),
                key_type: "certificate".into(),
                algorithm: None,
                private_key: test_private_key(),
                cert_data: Some("not-a-certificate".into()),
            })
            .unwrap_err();

        assert!(matches!(error, VaultError::InvalidSshKeyConfig(_)));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn accepts_matching_user_certificate() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();
        let mut seed =
            <rand_chacha::ChaCha20Rng as rand_chacha::rand_core::SeedableRng>::Seed::default();
        getrandom::fill(&mut seed).unwrap();
        let mut rng =
            <rand_chacha::ChaCha20Rng as rand_chacha::rand_core::SeedableRng>::from_seed(seed);
        let subject = PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let certificate_authority =
            PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let mut certificate_builder = ssh_key::certificate::Builder::new_with_random_nonce(
            &mut rng,
            subject.public_key(),
            0,
            u64::MAX,
        )
        .unwrap();
        certificate_builder.valid_principal("test-user").unwrap();
        let certificate = certificate_builder
            .sign(&certificate_authority)
            .unwrap()
            .to_openssh()
            .unwrap();
        let private_key = subject
            .to_openssh(ssh_key::LineEnding::LF)
            .unwrap()
            .to_string();

        let key = vault
            .create_key(&CreateKeyRequest {
                name: "Matching certificate".into(),
                key_type: "certificate".into(),
                algorithm: Some("ssh-ed25519".into()),
                private_key,
                cert_data: Some(certificate),
            })
            .unwrap();

        assert_eq!(key.algorithm, "ssh-ed25519");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_certificate_for_a_different_private_key() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();
        let mut seed =
            <rand_chacha::ChaCha20Rng as rand_chacha::rand_core::SeedableRng>::Seed::default();
        getrandom::fill(&mut seed).unwrap();
        let mut rng =
            <rand_chacha::ChaCha20Rng as rand_chacha::rand_core::SeedableRng>::from_seed(seed);
        let subject = PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let certificate_authority =
            PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let mut certificate_builder = ssh_key::certificate::Builder::new_with_random_nonce(
            &mut rng,
            subject.public_key(),
            0,
            u64::MAX,
        )
        .unwrap();
        certificate_builder.valid_principal("test-user").unwrap();
        let certificate = certificate_builder
            .sign(&certificate_authority)
            .unwrap()
            .to_openssh()
            .unwrap();

        let error = vault
            .create_key(&CreateKeyRequest {
                name: "Wrong certificate".into(),
                key_type: "certificate".into(),
                algorithm: None,
                private_key: test_private_key(),
                cert_data: Some(certificate),
            })
            .unwrap_err();

        assert!(matches!(error, VaultError::InvalidSshKeyConfig(_)));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovers_from_backup_when_primary_file_is_invalid() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();
        vault
            .create_key(&CreateKeyRequest {
                name: "Key".into(),
                key_type: "key".into(),
                algorithm: None,
                private_key: test_private_key(),
                cert_data: None,
            })
            .unwrap();
        vault
            .create_key(&CreateKeyRequest {
                name: "Later key".into(),
                key_type: "key".into(),
                algorithm: None,
                private_key: test_private_key(),
                cert_data: None,
            })
            .unwrap();
        drop(vault);

        fs::write(directory.join(VAULT_FILE_NAME), "not valid JSON").unwrap();
        let recovered = Vault::open_for_test(&directory).unwrap();
        assert_eq!(recovered.list_keys().unwrap().len(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn persists_two_hundred_profiles_with_a_shared_key() {
        let directory = test_dir();
        let vault = Vault::open_for_test(&directory).unwrap();
        let key = vault
            .create_key(&CreateKeyRequest {
                name: "Shared key".into(),
                key_type: "key".into(),
                algorithm: None,
                private_key: test_private_key(),
                cert_data: None,
            })
            .unwrap();
        for index in 0..200 {
            vault
                .create_profile(&CreateProfileRequest {
                    name: format!("Host {index}"),
                    host: format!("host-{index}.example.test"),
                    port: Some(22),
                    username: "root".into(),
                    auth_type: AuthType::Key,
                    credential: None,
                    key_id: Some(key.id.clone()),
                    proxy_id: None,
                    group_name: None,
                    icon: None,
                    color: None,
                    os: None,
                    location: None,
                })
                .unwrap();
        }
        drop(vault);

        let reopened = Vault::open_for_test(&directory).unwrap();
        let profiles = reopened.list_profiles().unwrap();
        assert_eq!(profiles.len(), 200);
        assert!(profiles
            .iter()
            .all(|profile| profile.key_id.as_deref() == Some(key.id.as_str())));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_plaintext_vault_files() {
        let directory = test_dir();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(VAULT_FILE_NAME), b"{\"formatVersion\":2}").unwrap();

        assert!(matches!(
            Vault::open_for_test(&directory),
            Err(VaultError::InvalidFormat(_))
        ));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_duplicate_or_invalid_scripts() {
        let directory = test_dir();
        let vault = Vault::open(&directory).unwrap();
        let request = CreateScriptRequest {
            name: "Disk usage".into(),
            description: None,
            tags: vec![],
            command: "df -h".into(),
            risk_level: super::super::models::ScriptRiskLevel::Low,
        };
        vault.create_script(&request).unwrap();
        assert!(vault.create_script(&request).is_err());
        assert!(vault
            .create_script(&CreateScriptRequest {
                name: "Bad".into(),
                description: None,
                tags: vec![],
                command: " ".into(),
                risk_level: super::super::models::ScriptRiskLevel::Low,
            })
            .is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn deleting_key_clears_profile_reference() {
        let directory = test_dir();
        let vault = Vault::open(&directory).unwrap();
        let key = vault
            .create_key(&CreateKeyRequest {
                name: "Key".into(),
                key_type: "key".into(),
                algorithm: None,
                private_key: test_private_key(),
                cert_data: None,
            })
            .unwrap();
        let profile = vault
            .create_profile(&CreateProfileRequest {
                name: "Host".into(),
                host: "host.test".into(),
                port: None,
                username: "root".into(),
                auth_type: AuthType::Key,
                credential: None,
                key_id: Some(key.id.clone()),
                proxy_id: None,
                group_name: None,
                icon: None,
                color: None,
                os: None,
                location: None,
            })
            .unwrap();
        vault.delete_key(&key.id).unwrap();
        assert_eq!(vault.get_profile(&profile.id).unwrap().key_id, None);
        fs::remove_dir_all(directory).unwrap();
    }
}
