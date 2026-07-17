use rusqlite::{params, Connection};
use std::path::Path;

use super::crypto::{self, MasterKey, KEY_LEN, SALT_LEN};
use super::models::{
    AiAgentConfig, AiProviderConfigSecret, AiProviderConfigView, AuthType, CreateKeyRequest,
    CreateProfileRequest, DecryptedCredential, SaveAiAgentConfigRequest,
    SaveAiProviderConfigRequest, SshKey, SshKeyView, SshProfile, UpdateProfileRequest,
};

/// local.key 格式: [16字节salt] + [32字节派生密钥]
const LOCAL_KEY_LEN: usize = SALT_LEN + KEY_LEN;
const DEFAULT_PASSWORD: &str = "LuckyMJJ";

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Crypto error: {0}")]
    Crypto(#[from] crypto::CryptoError),
    #[error("Vault not initialized, please setup master password")]
    NotInitialized,
    #[error("Wrong master password")]
    WrongPassword,
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
    #[error("Invalid AI configuration: {0}")]
    InvalidAiConfig(String),
    #[error("AI Agent not found: {0}")]
    AiAgentNotFound(String),
    #[error("The default AI Agent cannot be deleted")]
    DefaultAiAgentCannotBeDeleted,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Vault {
    conn: Connection,
    master_key: MasterKey,
}

impl Vault {
    /// 自动打开 vault（读取 local.key）
    pub fn open_auto(app_dir: &Path) -> Result<Self, VaultError> {
        let key_path = app_dir.join("local.key");
        let db_path = app_dir.join("vault.db");

        // 如果 local.key 不存在，用默认密码自动初始化
        if !key_path.exists() {
            return Self::setup(app_dir, DEFAULT_PASSWORD);
        }

        let key_data = std::fs::read(&key_path)?;
        if key_data.len() != LOCAL_KEY_LEN {
            return Err(VaultError::Crypto(crypto::CryptoError::InvalidFormat));
        }

        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&key_data[SALT_LEN..]);

        let conn = Connection::open(&db_path)?;
        Self::ensure_table(&conn)?;

        let vault = Self {
            conn,
            master_key: MasterKey { key },
        };
        vault.ensure_default_ai_agent()?;
        Ok(vault)
    }

    /// 检查是否使用默认密码（通过 config.init 验证）
    pub fn is_default_password(app_dir: &Path) -> bool {
        let db_path = app_dir.join("vault.db");
        if !db_path.exists() {
            return true;
        }

        let conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(_) => return true,
        };

        // 读取 config.init
        let init_data: Vec<u8> =
            match conn.query_row("SELECT value FROM config WHERE key = 'init'", [], |row| {
                row.get(0)
            }) {
                Ok(d) => d,
                Err(_) => return true,
            };

        // 用默认密码派生密钥，尝试解密 init
        let key_path = app_dir.join("local.key");
        let key_data = match std::fs::read(&key_path) {
            Ok(d) => d,
            Err(_) => return true,
        };
        if key_data.len() != LOCAL_KEY_LEN {
            return true;
        }

        let salt = &key_data[..SALT_LEN];
        let stored_key = &key_data[SALT_LEN..];

        // 用默认密码派生的密钥解密 init
        match crypto::derive_key(DEFAULT_PASSWORD.as_bytes(), salt) {
            Ok(derived_key) => {
                if derived_key != *stored_key {
                    return false;
                }
                // 密钥匹配，尝试解密 init
                crypto::decrypt_string(&derived_key, &init_data).is_ok()
            }
            Err(_) => false,
        }
    }

    /// 首次设置：用主密码初始化 vault
    pub fn setup(app_dir: &Path, password: &str) -> Result<Self, VaultError> {
        let db_path = app_dir.join("vault.db");
        let key_path = app_dir.join("local.key");

        // 生成 salt + 派生密钥
        let salt = crypto::generate_salt();
        let key = crypto::derive_key(password.as_bytes(), &salt)?;

        // 存储 [salt][key] 到 local.key
        let mut key_data = Vec::with_capacity(LOCAL_KEY_LEN);
        key_data.extend_from_slice(&salt);
        key_data.extend_from_slice(&key);
        std::fs::write(&key_path, &key_data)?;

        // 打开/创建数据库
        let conn = Connection::open(&db_path)?;
        Self::ensure_table(&conn)?;

        // 写入 config.init（加密的 "LuckyMJJ"）
        let encrypted_init = crypto::encrypt_string(&key, DEFAULT_PASSWORD)?;
        conn.execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES ('init', ?1)",
            [encrypted_init],
        )?;

        let vault = Self {
            conn,
            master_key: MasterKey { key },
        };
        vault.ensure_default_ai_agent()?;
        Ok(vault)
    }

    /// 验证主密码（用于修改密码前验证）
    pub fn verify_password(app_dir: &Path, password: &str) -> Result<bool, VaultError> {
        let key_path = app_dir.join("local.key");
        if !key_path.exists() {
            return Err(VaultError::NotInitialized);
        }

        let key_data = std::fs::read(&key_path)?;
        if key_data.len() != LOCAL_KEY_LEN {
            return Err(VaultError::Crypto(crypto::CryptoError::InvalidFormat));
        }

        let salt = &key_data[..SALT_LEN];
        let stored_key = &key_data[SALT_LEN..];

        let derived_key = crypto::derive_key(password.as_bytes(), salt)?;
        Ok(derived_key == stored_key)
    }

    /// 修改主密码（重新派生密钥，重新加密所有凭证）
    pub fn change_password(
        app_dir: &Path,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), VaultError> {
        // 验证旧密码
        if !Self::verify_password(app_dir, old_password)? {
            return Err(VaultError::WrongPassword);
        }

        let db_path = app_dir.join("vault.db");
        let key_path = app_dir.join("local.key");

        // 读取旧密钥
        let old_key_data = std::fs::read(&key_path)?;
        let mut old_key = [0u8; KEY_LEN];
        old_key.copy_from_slice(&old_key_data[SALT_LEN..]);

        // 生成新 salt + 新密钥
        let new_salt = crypto::generate_salt();
        let new_key = crypto::derive_key(new_password.as_bytes(), &new_salt)?;

        // 打开数据库
        let conn = Connection::open(&db_path)?;

        // 重新加密 profiles.credential
        let profiles = Self::read_all_profiles(&conn)?;
        for profile in &profiles {
            let cred = match profile.auth_type {
                AuthType::Password => match &profile.credential {
                    Some(data) => {
                        let decrypted = crypto::decrypt_string(&old_key, data)?;
                        Some(crypto::encrypt_string(&new_key, &decrypted)?)
                    }
                    None => None,
                },
                _ => profile.credential.clone(),
            };
            conn.execute(
                "UPDATE profiles SET credential = ?1 WHERE id = ?2",
                params![cred, profile.id],
            )?;
        }

        // 重新加密 ssh_keys.private_key 和 ssh_keys.cert_data
        let mut stmt = conn.prepare("SELECT id, private_key, cert_data FROM ssh_keys")?;
        let keys: Vec<(String, Vec<u8>, Option<Vec<u8>>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        for (key_id, private_key, cert_data) in &keys {
            let decrypted_key = crypto::decrypt_string(&old_key, private_key)?;
            let new_private_key = crypto::encrypt_string(&new_key, &decrypted_key)?;

            let new_cert = match cert_data {
                Some(data) => {
                    let decrypted = crypto::decrypt_string(&old_key, data)?;
                    Some(crypto::encrypt_string(&new_key, &decrypted)?)
                }
                None => None,
            };

            conn.execute(
                "UPDATE ssh_keys SET private_key = ?1, cert_data = ?2 WHERE id = ?3",
                params![new_private_key, new_cert, key_id],
            )?;
        }

        // 重新加密 AI provider API Key
        let mut stmt = conn.prepare("SELECT id, api_key FROM ai_provider_configs")?;
        let ai_configs: Vec<(String, Vec<u8>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, api_key) in ai_configs {
            let decrypted_key = crypto::decrypt_string(&old_key, &api_key)?;
            let encrypted_key = crypto::encrypt_string(&new_key, &decrypted_key)?;
            conn.execute(
                "UPDATE ai_provider_configs SET api_key = ?1 WHERE id = ?2",
                params![encrypted_key, id],
            )?;
        }

        // 重新加密 AI Agent 提示词
        let mut stmt = conn.prepare("SELECT id, prompt FROM ai_agent_configs")?;
        let agents: Vec<(String, Vec<u8>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, prompt) in agents {
            let decrypted_prompt = crypto::decrypt_string(&old_key, &prompt)?;
            let encrypted_prompt = crypto::encrypt_string(&new_key, &decrypted_prompt)?;
            conn.execute(
                "UPDATE ai_agent_configs SET prompt = ?1 WHERE id = ?2",
                params![encrypted_prompt, id],
            )?;
        }

        // 写入新 local.key
        let mut new_key_data = Vec::with_capacity(LOCAL_KEY_LEN);
        new_key_data.extend_from_slice(&new_salt);
        new_key_data.extend_from_slice(&new_key);
        std::fs::write(&key_path, &new_key_data)?;

        // 更新 config.init（用新密钥加密默认密码）
        let new_encrypted_init = crypto::encrypt_string(&new_key, DEFAULT_PASSWORD)?;
        conn.execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES ('init', ?1)",
            [new_encrypted_init],
        )?;

        Ok(())
    }

    pub fn get_ai_config_view(&self) -> Result<AiProviderConfigView, VaultError> {
        self.conn
            .query_row(
                "SELECT base_url, model, timeout_seconds FROM ai_provider_configs ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| {
                    Ok(AiProviderConfigView {
                        configured: true,
                        provider_type: "openai_compatible".into(),
                        base_url: Some(row.get(0)?),
                        model: Some(row.get(1)?),
                        timeout_seconds: Some(row.get(2)?),
                    })
                },
            )
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(AiProviderConfigView {
                    configured: false,
                    provider_type: "openai_compatible".into(),
                    base_url: None,
                    model: None,
                    timeout_seconds: None,
                }),
                other => Err(VaultError::Database(other)),
            })
    }

    pub fn save_ai_config(&self, request: &SaveAiProviderConfigRequest) -> Result<(), VaultError> {
        let base_url = request.base_url.trim().trim_end_matches('/');
        if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
            return Err(VaultError::InvalidAiConfig(
                "base_url must start with http:// or https://".into(),
            ));
        }
        if request.model.trim().is_empty() {
            return Err(VaultError::InvalidAiConfig("model is required".into()));
        }
        if !(10..=300).contains(&request.timeout_seconds) {
            return Err(VaultError::InvalidAiConfig(
                "timeout_seconds must be between 10 and 300".into(),
            ));
        }

        let api_key = if request.api_key.trim().is_empty() {
            self.conn
                .query_row(
                    "SELECT api_key FROM ai_provider_configs WHERE id = 'default'",
                    [],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .map_err(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => VaultError::InvalidAiConfig(
                        "api_key is required when creating a configuration".into(),
                    ),
                    other => VaultError::Database(other),
                })?
        } else {
            crypto::encrypt_string(&self.master_key.key, request.api_key.trim())?
        };
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO ai_provider_configs (id, provider_type, base_url, api_key, model, timeout_seconds, created_at, updated_at)
             VALUES ('default', 'openai_compatible', ?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET base_url = excluded.base_url, api_key = excluded.api_key,
               model = excluded.model, timeout_seconds = excluded.timeout_seconds, updated_at = excluded.updated_at",
            params![base_url, api_key, request.model.trim(), request.timeout_seconds, now],
        )?;
        Ok(())
    }

    pub fn get_ai_config_secret(&self) -> Result<Option<AiProviderConfigSecret>, VaultError> {
        self.conn
            .query_row(
                "SELECT base_url, api_key, model, timeout_seconds FROM ai_provider_configs WHERE id = 'default'",
                [],
                |row| {
                    let encrypted_api_key: Vec<u8> = row.get(1)?;
                    let api_key = crypto::decrypt_string(&self.master_key.key, &encrypted_api_key)
                        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                    Ok(AiProviderConfigSecret {
                        base_url: row.get(0)?,
                        api_key,
                        model: row.get(2)?,
                        timeout_seconds: row.get(3)?,
                    })
                },
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(VaultError::Database(other)),
            })
    }

    pub fn has_ai_executable_grant(
        &self,
        executable: &str,
        server_key: &str,
    ) -> Result<bool, VaultError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM ai_executable_grants
             WHERE executable_name = ?1
               AND ((scope = 'global' AND scope_target = '')
                 OR (scope = 'server' AND scope_target = ?2))",
            params![executable, server_key],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn save_ai_executable_grant(
        &self,
        executable: &str,
        scope: &str,
        scope_target: &str,
    ) -> Result<(), VaultError> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO ai_executable_grants (executable_name, scope, scope_target, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(executable_name, scope, scope_target) DO UPDATE SET updated_at = excluded.updated_at",
            params![executable, scope, scope_target, now],
        )?;
        Ok(())
    }

    pub fn delete_ai_config(&self) -> Result<(), VaultError> {
        self.conn.execute("DELETE FROM ai_provider_configs", [])?;
        Ok(())
    }

    pub fn list_ai_agents(&self) -> Result<Vec<AiAgentConfig>, VaultError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, prompt, is_default, created_at, updated_at
             FROM ai_agent_configs ORDER BY is_default DESC, name COLLATE NOCASE",
        )?;
        let agents = stmt
            .query_map([], |row| self.decrypt_ai_agent(row))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(VaultError::Database)?;
        Ok(agents)
    }

    pub fn get_ai_agent(&self, id: &str) -> Result<AiAgentConfig, VaultError> {
        self.conn
            .query_row(
                "SELECT id, name, prompt, is_default, created_at, updated_at
                 FROM ai_agent_configs WHERE id = ?1",
                [id],
                |row| self.decrypt_ai_agent(row),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => VaultError::AiAgentNotFound(id.to_owned()),
                other => VaultError::Database(other),
            })
    }

    pub fn get_default_ai_agent(&self) -> Result<AiAgentConfig, VaultError> {
        self.conn
            .query_row(
                "SELECT id, name, prompt, is_default, created_at, updated_at
                 FROM ai_agent_configs WHERE is_default = 1 LIMIT 1",
                [],
                |row| self.decrypt_ai_agent(row),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    VaultError::AiAgentNotFound("default".into())
                }
                other => VaultError::Database(other),
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

        let id = request
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let now = chrono::Utc::now().to_rfc3339();
        let encrypted_prompt = crypto::encrypt_string(&self.master_key.key, prompt)?;
        self.conn.execute(
            "INSERT INTO ai_agent_configs (id, name, prompt, is_default, created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, prompt = excluded.prompt,
               updated_at = excluded.updated_at",
            params![id, name, encrypted_prompt, now],
        )?;
        self.get_ai_agent(&id)
    }

    pub fn delete_ai_agent(&self, id: &str) -> Result<(), VaultError> {
        if self.get_ai_agent(id)?.is_default {
            return Err(VaultError::DefaultAiAgentCannotBeDeleted);
        }
        let rows = self
            .conn
            .execute("DELETE FROM ai_agent_configs WHERE id = ?1", [id])?;
        if rows == 0 {
            return Err(VaultError::AiAgentNotFound(id.to_owned()));
        }
        Ok(())
    }

    fn ensure_default_ai_agent(&self) -> Result<(), VaultError> {
        let prompt = r#"# Role: MJJ Agent (高级远程运维专家 & SSH 智能助手)

## 角色定位与核心能力
你是一位拥有十年以上经验的资深 Linux/Unix 运维专家。协助用户管理服务器、排查故障、部署服务并优化系统性能。你精通 Ubuntu、Debian、CentOS、Alpine、Nginx、Apache、Docker、Kubernetes、MySQL、PostgreSQL 及网络安全配置。

## 沟通方式
根据用户的技术水平调整表达。对初学者使用清晰的中文分步骤说明，并说明每步的预期结果；对有经验的运维人员先给出诊断结论，使用简洁准确的术语。使用 Markdown 标题、列表和代码块组织答案。"#;
        let encrypted_prompt = crypto::encrypt_string(&self.master_key.key, prompt)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR IGNORE INTO ai_agent_configs (id, name, prompt, is_default, created_at, updated_at)
             VALUES ('mjj-agent', 'MJJ Agent', ?1, 1, ?2, ?2)",
            params![encrypted_prompt, now],
        )?;
        Ok(())
    }

    fn decrypt_ai_agent(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<AiAgentConfig> {
        let encrypted_prompt: Vec<u8> = row.get(2)?;
        let prompt =
            crypto::decrypt_string(&self.master_key.key, &encrypted_prompt).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Blob,
                    Box::new(error),
                )
            })?;
        Ok(AiAgentConfig {
            id: row.get(0)?,
            name: row.get(1)?,
            prompt,
            is_default: row.get::<_, i64>(3)? != 0,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }

    fn ensure_table(conn: &Connection) -> Result<(), VaultError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL,
                auth_type TEXT NOT NULL,
                credential BLOB,
                private_key BLOB,
                cert_data BLOB,
                key_id TEXT,
                group_name TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ssh_keys (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                key_type TEXT NOT NULL,
                private_key BLOB NOT NULL,
                cert_data BLOB,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ai_provider_configs (
                id TEXT PRIMARY KEY,
                provider_type TEXT NOT NULL,
                base_url TEXT NOT NULL,
                api_key BLOB NOT NULL,
                model TEXT NOT NULL,
                timeout_seconds INTEGER NOT NULL DEFAULT 60,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ai_agent_configs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                prompt BLOB NOT NULL,
                is_default INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );


            CREATE TABLE IF NOT EXISTS ai_executable_grants (
                executable_name TEXT NOT NULL,
                scope TEXT NOT NULL CHECK (scope IN ('server', 'global')),
                scope_target TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (executable_name, scope, scope_target)
            );",
        )?;

        Ok(())
    }

    fn read_all_profiles(conn: &Connection) -> Result<Vec<SshProfile>, VaultError> {
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, username, auth_type, credential, private_key, cert_data, key_id, group_name, created_at, updated_at
             FROM profiles",
        )?;

        let profiles = stmt
            .query_map([], |row| {
                Ok(SshProfile {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    host: row.get(2)?,
                    port: row.get(3)?,
                    username: row.get(4)?,
                    auth_type: match row.get::<_, String>(5)?.as_str() {
                        "password" => AuthType::Password,
                        "key" => AuthType::Key,
                        "certificate" => AuthType::Certificate,
                        _ => AuthType::Password,
                    },
                    credential: row.get(6)?,
                    private_key: row.get(7)?,
                    cert_data: row.get(8)?,
                    key_id: row.get(9)?,
                    group_name: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(profiles)
    }

    pub fn list_profiles(&self) -> Result<Vec<SshProfile>, VaultError> {
        Self::read_all_profiles(&self.conn)
    }

    pub fn get_profile(&self, id: &str) -> Result<SshProfile, VaultError> {
        self.conn
            .query_row(
                "SELECT id, name, host, port, username, auth_type, credential, private_key, cert_data, key_id, group_name, created_at, updated_at
                 FROM profiles WHERE id = ?1",
                [id],
                |row| {
                    Ok(SshProfile {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        host: row.get(2)?,
                        port: row.get(3)?,
                        username: row.get(4)?,
                        auth_type: match row.get::<_, String>(5)?.as_str() {
                            "password" => AuthType::Password,
                            "key" => AuthType::Key,
                            "certificate" => AuthType::Certificate,
                            _ => AuthType::Password,
                        },
                        credential: row.get(6)?,
                        private_key: row.get(7)?,
                        cert_data: row.get(8)?,
                        key_id: row.get(9)?,
                        group_name: row.get(10)?,
                        created_at: row.get(11)?,
                        updated_at: row.get(12)?,
                    })
                },
            )
            .map_err(|_| VaultError::ProfileNotFound(id.to_string()))
    }

    pub fn create_profile(&self, req: &CreateProfileRequest) -> Result<SshProfile, VaultError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let port = req.port.unwrap_or(22);

        let encrypted_credential = match &req.credential {
            Some(c) if !c.is_empty() => Some(crypto::encrypt_string(&self.master_key.key, c)?),
            _ => None,
        };

        self.conn.execute(
            "INSERT INTO profiles (id, name, host, port, username, auth_type, credential, key_id, group_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                req.name,
                req.host,
                port,
                req.username,
                req.auth_type.to_string(),
                encrypted_credential,
                req.key_id,
                req.group_name,
                now,
                now,
            ],
        )?;

        self.get_profile(&id)
    }

    pub fn update_profile(
        &self,
        id: &str,
        req: &UpdateProfileRequest,
    ) -> Result<SshProfile, VaultError> {
        let existing = self.get_profile(id)?;
        let now = chrono::Utc::now().to_rfc3339();

        let name = req.name.as_deref().unwrap_or(&existing.name);
        let host = req.host.as_deref().unwrap_or(&existing.host);
        let port = req.port.unwrap_or(existing.port);
        let username = req.username.as_deref().unwrap_or(&existing.username);
        let auth_type = req.auth_type.as_ref().unwrap_or(&existing.auth_type);
        let key_id = req.key_id.as_deref().or(existing.key_id.as_deref());
        let group_name = req.group_name.as_deref().or(existing.group_name.as_deref());

        let credential = match &req.credential {
            Some(c) if !c.is_empty() => Some(crypto::encrypt_string(&self.master_key.key, c)?),
            Some(_) => None,
            None => existing.credential,
        };

        self.conn.execute(
            "UPDATE profiles SET name = ?1, host = ?2, port = ?3, username = ?4, auth_type = ?5, credential = ?6, key_id = ?7, group_name = ?8, updated_at = ?9
             WHERE id = ?10",
            params![name, host, port, username, auth_type.to_string(), credential, key_id, group_name, now, id],
        )?;

        self.get_profile(id)
    }

    pub fn delete_profile(&self, id: &str) -> Result<(), VaultError> {
        let rows = self
            .conn
            .execute("DELETE FROM profiles WHERE id = ?1", [id])?;
        if rows == 0 {
            return Err(VaultError::ProfileNotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn decrypt_credential(
        &self,
        profile: &SshProfile,
    ) -> Result<DecryptedCredential, VaultError> {
        // 如果有关联的 key_id，从 ssh_keys 表读取
        if let Some(ref key_id) = profile.key_id {
            let key = self.get_key(key_id)?;
            let private_key = Some(crypto::decrypt_string(
                &self.master_key.key,
                &key.private_key,
            )?);
            let cert_data = key
                .cert_data
                .map(|d| crypto::decrypt_string(&self.master_key.key, &d))
                .transpose()?;
            let password = match profile.auth_type {
                AuthType::Password => profile
                    .credential
                    .as_ref()
                    .map(|c| crypto::decrypt_string(&self.master_key.key, c))
                    .transpose()?,
                _ => None,
            };
            return Ok(DecryptedCredential {
                password,
                private_key,
                cert_data,
            });
        }

        // 兼容旧格式：直接从 profile 字段读取
        let password = match profile.auth_type {
            AuthType::Password => profile
                .credential
                .as_ref()
                .map(|c| crypto::decrypt_string(&self.master_key.key, c))
                .transpose()?,
            _ => None,
        };

        let private_key = profile
            .private_key
            .as_ref()
            .map(|d| crypto::decrypt_string(&self.master_key.key, d))
            .transpose()?;
        let cert_data = profile
            .cert_data
            .as_ref()
            .map(|d| crypto::decrypt_string(&self.master_key.key, d))
            .transpose()?;

        Ok(DecryptedCredential {
            password,
            private_key,
            cert_data,
        })
    }

    // ==================== SSH Keys ====================

    pub fn list_keys(&self) -> Result<Vec<SshKeyView>, VaultError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, key_type, created_at, updated_at FROM ssh_keys ORDER BY name",
        )?;

        let keys = stmt
            .query_map([], |row| {
                Ok(SshKeyView {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    key_type: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(keys)
    }

    pub fn get_key(&self, id: &str) -> Result<SshKey, VaultError> {
        self.conn
            .query_row(
                "SELECT id, name, key_type, private_key, cert_data, created_at, updated_at
                 FROM ssh_keys WHERE id = ?1",
                [id],
                |row| {
                    Ok(SshKey {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        key_type: row.get(2)?,
                        private_key: row.get(3)?,
                        cert_data: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .map_err(|_| VaultError::ProfileNotFound(format!("Key not found: {}", id)))
    }

    pub fn create_key(&self, req: &CreateKeyRequest) -> Result<SshKeyView, VaultError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let encrypted_key = crypto::encrypt_string(&self.master_key.key, &req.private_key)?;
        let encrypted_cert = match &req.cert_data {
            Some(c) if !c.is_empty() => Some(crypto::encrypt_string(&self.master_key.key, c)?),
            _ => None,
        };

        self.conn.execute(
            "INSERT INTO ssh_keys (id, name, key_type, private_key, cert_data, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, req.name, req.key_type, encrypted_key, encrypted_cert, now, now],
        )?;

        Ok(SshKeyView {
            id,
            name: req.name.clone(),
            key_type: req.key_type.clone(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn update_key(&self, id: &str, req: &CreateKeyRequest) -> Result<SshKeyView, VaultError> {
        let now = chrono::Utc::now().to_rfc3339();

        let encrypted_key = crypto::encrypt_string(&self.master_key.key, &req.private_key)?;
        let encrypted_cert = match &req.cert_data {
            Some(c) if !c.is_empty() => Some(crypto::encrypt_string(&self.master_key.key, c)?),
            _ => None,
        };

        self.conn.execute(
            "UPDATE ssh_keys SET name = ?1, key_type = ?2, private_key = ?3, cert_data = ?4, updated_at = ?5
             WHERE id = ?6",
            params![req.name, req.key_type, encrypted_key, encrypted_cert, now, id],
        )?;

        Ok(SshKeyView {
            id: id.to_string(),
            name: req.name.clone(),
            key_type: req.key_type.clone(),
            updated_at: now,
            ..Default::default()
        })
    }

    pub fn delete_key(&self, id: &str) -> Result<(), VaultError> {
        // 删除关联的 profile 中的 key_id 引用
        self.conn
            .execute("UPDATE profiles SET key_id = NULL WHERE key_id = ?1", [id])?;
        self.conn
            .execute("DELETE FROM ssh_keys WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("my-ssh-vault-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn updating_ai_config_without_an_api_key_preserves_the_existing_key() {
        let app_dir = test_app_dir();
        std::fs::create_dir_all(&app_dir).unwrap();
        let vault = Vault::setup(&app_dir, "test-password").unwrap();

        vault
            .save_ai_config(&SaveAiProviderConfigRequest {
                base_url: "https://ctmoai.com".into(),
                api_key: "test-api-key".into(),
                model: "gpt-5.6-terra".into(),
                timeout_seconds: 60,
            })
            .unwrap();
        vault
            .save_ai_config(&SaveAiProviderConfigRequest {
                base_url: "https://ctmoai.com/v1".into(),
                api_key: String::new(),
                model: "gpt-5.6-terra".into(),
                timeout_seconds: 90,
            })
            .unwrap();

        let config = vault.get_ai_config_secret().unwrap().unwrap();
        assert_eq!(config.base_url, "https://ctmoai.com/v1");
        assert_eq!(config.api_key, "test-api-key");
        assert_eq!(config.timeout_seconds, 90);

        drop(vault);
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn ai_agent_prompt_is_encrypted_and_survives_password_change() {
        let app_dir = test_app_dir();
        std::fs::create_dir_all(&app_dir).unwrap();
        let prompt = "Respond only with evidence-based diagnostics.";

        let vault = Vault::setup(&app_dir, "old-password").unwrap();
        let default_agent = vault.get_default_ai_agent().unwrap();
        assert_eq!(default_agent.id, "mjj-agent");
        assert!(matches!(
            vault.delete_ai_agent(&default_agent.id),
            Err(VaultError::DefaultAiAgentCannotBeDeleted)
        ));

        let agent = vault
            .save_ai_agent(&SaveAiAgentConfigRequest {
                id: None,
                name: "Test Agent".into(),
                prompt: prompt.into(),
            })
            .unwrap();
        let encrypted_prompt: Vec<u8> = vault
            .conn
            .query_row(
                "SELECT prompt FROM ai_agent_configs WHERE id = ?1",
                [&agent.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!encrypted_prompt
            .windows(prompt.len())
            .any(|window| window == prompt.as_bytes()));
        assert_eq!(vault.get_ai_agent(&agent.id).unwrap().prompt, prompt);
        drop(vault);

        Vault::change_password(&app_dir, "old-password", "new-password").unwrap();
        let vault = Vault::open_auto(&app_dir).unwrap();
        assert_eq!(vault.get_ai_agent(&agent.id).unwrap().prompt, prompt);
        drop(vault);
        std::fs::remove_dir_all(app_dir).unwrap();
    }
}
