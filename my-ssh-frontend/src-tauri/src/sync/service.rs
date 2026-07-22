use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use zeroize::Zeroize;

use crate::vault::{Vault, VaultError, VaultSyncSnapshot};

use super::gitee_snippet::{GiteeSnippetError, GiteeSnippetRemote};
use super::github_gist::{GithubGistError, GithubGistRemote, GIST_FILE_NAME};
use super::models::RemoteDocument;
use super::state::{SyncState, SyncStateError, SyncStateStore};
use super::{
    decrypt_vault, decrypt_vault_with_key, derive_sync_key, encrypt_vault, encrypt_vault_with_key,
    EncryptedVault, SyncCryptoError,
};

pub const GITHUB_GIST_PROVIDER: &str = "github_gist";
pub const GITEE_SNIPPET_PROVIDER: &str = "gitee_snippet";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncProvider {
    GithubGist,
    GiteeSnippet,
}

impl SyncProvider {
    pub fn parse(value: &str) -> Result<Self, SyncServiceError> {
        match value {
            GITHUB_GIST_PROVIDER => Ok(Self::GithubGist),
            GITEE_SNIPPET_PROVIDER => Ok(Self::GiteeSnippet),
            _ => Err(SyncServiceError::UnsupportedProvider),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::GithubGist => GITHUB_GIST_PROVIDER,
            Self::GiteeSnippet => GITEE_SNIPPET_PROVIDER,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyncServiceError {
    #[error("cloud sync is not configured")]
    NotConfigured,
    #[error("cloud sync provider is unsupported")]
    UnsupportedProvider,
    #[error("cloud sync conflict: local or remote data changed since the last sync")]
    Conflict,
    #[error("multiple MJJSSH cloud sync vaults were found; delete duplicate private snippets, then try again")]
    MultipleSyncVaults,
    #[error("sync password is incorrect or sync data is corrupted")]
    InvalidRemoteData,
    #[error("cloud sync storage error: {0}")]
    State(#[from] SyncStateError),
    #[error("local Vault error: {0}")]
    Vault(#[from] VaultError),
    #[error("GitHub sync error: {0}")]
    Github(#[from] GithubGistError),
    #[error("Gitee sync error: {0}")]
    Gitee(#[from] GiteeSnippetError),
    #[error("sync encryption error: {0}")]
    Crypto(#[from] SyncCryptoError),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub configured: bool,
    pub provider: Option<String>,
    pub remote_id: Option<String>,
    pub remote_file_name: Option<String>,
    pub state: String,
    pub last_synced_at: Option<String>,
    pub device_id: Option<String>,
    pub token: Option<String>,
    pub auto_sync: bool,
    pub local_vault_revision: Option<u64>,
    pub last_synced_vault_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOperationResult {
    pub status: String,
    pub sync: SyncStatus,
}

pub struct SyncService<'a> {
    vault: &'a Vault,
    app_dir: PathBuf,
    state_store: SyncStateStore,
}

enum Remote {
    Github(GithubGistRemote),
    Gitee(GiteeSnippetRemote),
}

impl Remote {
    async fn get(&self, token: &str, remote_id: &str) -> Result<RemoteDocument, SyncServiceError> {
        match self {
            Self::Github(remote) => Ok(remote.get(token, remote_id).await?),
            Self::Gitee(remote) => Ok(remote.get(token, remote_id).await?),
        }
    }

    async fn find_sync_vaults(&self, token: &str) -> Result<Vec<RemoteDocument>, SyncServiceError> {
        match self {
            Self::Github(remote) => Ok(remote.find_sync_vaults(token).await?),
            Self::Gitee(remote) => Ok(remote.find_sync_vaults(token).await?),
        }
    }

    async fn create(&self, token: &str, content: &str) -> Result<RemoteDocument, SyncServiceError> {
        match self {
            Self::Github(remote) => Ok(remote.create(token, content).await?),
            Self::Gitee(remote) => Ok(remote.create(token, content).await?),
        }
    }

    async fn update(
        &self,
        token: &str,
        remote_id: &str,
        content: &str,
    ) -> Result<RemoteDocument, SyncServiceError> {
        match self {
            Self::Github(remote) => Ok(remote.update(token, remote_id, content).await?),
            Self::Gitee(remote) => Ok(remote.update(token, remote_id, content).await?),
        }
    }

    async fn delete(&self, token: &str, remote_id: &str) -> Result<(), SyncServiceError> {
        match self {
            Self::Github(remote) => Ok(remote.delete(token, remote_id).await?),
            Self::Gitee(remote) => Ok(remote.delete(token, remote_id).await?),
        }
    }
}

impl<'a> SyncService<'a> {
    pub fn new(vault: &'a Vault, app_dir: &Path) -> Result<Self, SyncServiceError> {
        Ok(Self {
            vault,
            app_dir: app_dir.into(),
            state_store: SyncStateStore::new(app_dir),
        })
    }

    pub fn status(&self) -> Result<SyncStatus, SyncServiceError> {
        let state = self.state_store.load()?;
        let mut status = status_from_state(state);
        if status.configured {
            status.local_vault_revision = Some(self.vault.sync_snapshot()?.revision);
        }
        Ok(status)
    }

    pub fn disable(&self) -> Result<(), SyncServiceError> {
        self.state_store.clear()?;
        Ok(())
    }

    pub fn set_auto_sync(&self, auto_sync: bool) -> Result<SyncStatus, SyncServiceError> {
        let mut state = self.configured_state()?;
        state.auto_sync = auto_sync;
        self.state_store.save(&state)?;
        Ok(status_from_state(Some(state)))
    }

    pub async fn update_local_password(
        &self,
        token: &str,
        password: String,
    ) -> Result<SyncStatus, SyncServiceError> {
        let mut state = self.configured_state()?;
        let remote = self
            .remote_for_state(&state)?
            .get(token, &state.remote_id)
            .await?;
        let envelope = parse_envelope(&remote)?;
        state.derived_sync_key = derive_key_for_envelope(&envelope, password)?;
        let key = saved_key(&state)?;
        decrypt_vault_with_key(&envelope, &key).map_err(map_crypto_error)?;
        self.state_store.save(&state)?;
        Ok(status_from_state(Some(state)))
    }

    pub async fn enable_create(
        &self,
        provider: SyncProvider,
        token: &str,
        password: String,
    ) -> Result<SyncStatus, SyncServiceError> {
        if self.state_store.load()?.is_some() {
            return Err(SyncServiceError::Conflict);
        }
        let snapshot = self.vault.sync_snapshot()?;
        let device_id = uuid::Uuid::new_v4().to_string();
        let encrypted = encrypt_snapshot(snapshot, password.clone(), device_id.clone())?;
        let derived_sync_key = derive_key_for_envelope(&encrypted, password)?;
        let remote_client = self.remote(provider)?;
        let matching = remote_client.find_sync_vaults(token).await?;
        let remote = match matching.as_slice() {
            [] => {
                remote_client
                    .create(token, &serialize_envelope(&encrypted)?)
                    .await?
            }
            [existing] => {
                remote_client
                    .update(token, &existing.remote_id, &serialize_envelope(&encrypted)?)
                    .await?
            }
            _ => return Err(SyncServiceError::MultipleSyncVaults),
        };
        let state = state_from_remote(
            provider,
            &snapshot_metadata(&encrypted),
            &remote,
            device_id,
            token.to_owned(),
            derived_sync_key,
            true,
        );
        self.state_store.save(&state)?;
        Ok(status_from_state(Some(state)))
    }

    pub async fn enable_or_import(
        &self,
        provider: SyncProvider,
        token: &str,
        password: String,
    ) -> Result<SyncStatus, SyncServiceError> {
        if self.state_store.load()?.is_some() {
            return Err(SyncServiceError::Conflict);
        }
        let remote = self.remote(provider)?;
        match remote.find_sync_vaults(token).await?.as_slice() {
            [] => self.enable_create(provider, token, password).await,
            [document] => self.import_document(provider, token, password, document),
            _ => Err(SyncServiceError::MultipleSyncVaults),
        }
    }

    fn import_document(
        &self,
        provider: SyncProvider,
        token: &str,
        password: String,
        remote: &RemoteDocument,
    ) -> Result<SyncStatus, SyncServiceError> {
        let envelope = parse_envelope(remote)?;
        let content = decrypt_vault(&envelope, password.clone()).map_err(map_crypto_error)?;
        let derived_sync_key = derive_key_for_envelope(&envelope, password)?;
        self.vault.replace_from_sync(&content)?;
        let state = state_from_remote(
            provider,
            &snapshot_metadata(&envelope),
            remote,
            uuid::Uuid::new_v4().to_string(),
            token.to_owned(),
            derived_sync_key,
            true,
        );
        self.state_store.save(&state)?;
        Ok(status_from_state(Some(state)))
    }

    pub async fn upload(&self, token: &str) -> Result<SyncOperationResult, SyncServiceError> {
        let state = self.configured_state()?;
        let snapshot = self.vault.sync_snapshot()?;
        let remote_client = self.remote_for_state(&state)?;
        let current = remote_client.get(token, &state.remote_id).await?;
        let key = saved_key(&state)?;
        let current_envelope = verify_remote_with_key(&current, &key)?;
        if current.content_hash != state.last_synced_content_hash {
            return Err(SyncServiceError::Conflict);
        }
        let salt = current_envelope
            .encryption
            .validate()
            .map_err(|_| SyncServiceError::InvalidRemoteData)?
            .0;
        let encrypted = encrypt_snapshot_with_key(snapshot, &key, state.device_id.clone(), salt)?;
        let remote = remote_client
            .update(token, &state.remote_id, &serialize_envelope(&encrypted)?)
            .await?;
        let state = state_from_remote(
            SyncProvider::parse(&state.provider)?,
            &snapshot_metadata(&encrypted),
            &remote,
            state.device_id,
            token.to_owned(),
            state.derived_sync_key,
            state.auto_sync,
        );
        self.state_store.save(&state)?;
        Ok(SyncOperationResult {
            status: "uploaded".into(),
            sync: status_from_state(Some(state)),
        })
    }

    pub async fn change_password(
        &self,
        token: &str,
        current_password: String,
        new_password: String,
    ) -> Result<SyncOperationResult, SyncServiceError> {
        let state = self.configured_state()?;
        let snapshot = self.vault.sync_snapshot()?;
        if snapshot.revision != state.last_synced_vault_revision {
            return Err(SyncServiceError::Conflict);
        }
        let remote_client = self.remote_for_state(&state)?;
        let current = remote_client.get(token, &state.remote_id).await?;
        let saved_key = saved_key(&state)?;
        let envelope = verify_remote_with_key(&current, &saved_key)?;
        if current.content_hash != state.last_synced_content_hash {
            return Err(SyncServiceError::Conflict);
        }
        let remote_content =
            decrypt_vault(&envelope, current_password).map_err(map_crypto_error)?;
        if remote_content != snapshot.content {
            return Err(SyncServiceError::Conflict);
        }
        let encrypted = encrypt_snapshot(snapshot, new_password.clone(), state.device_id.clone())?;
        let derived_sync_key = derive_key_for_envelope(&encrypted, new_password)?;
        let remote = remote_client
            .update(token, &state.remote_id, &serialize_envelope(&encrypted)?)
            .await?;
        let state = state_from_remote(
            SyncProvider::parse(&state.provider)?,
            &snapshot_metadata(&encrypted),
            &remote,
            state.device_id,
            token.to_owned(),
            derived_sync_key,
            state.auto_sync,
        );
        self.state_store.save(&state)?;
        Ok(SyncOperationResult {
            status: "password_changed".into(),
            sync: status_from_state(Some(state)),
        })
    }

    pub async fn download(&self, token: &str) -> Result<SyncOperationResult, SyncServiceError> {
        let state = self.configured_state()?;
        let remote = self
            .remote_for_state(&state)?
            .get(token, &state.remote_id)
            .await?;
        let key = saved_key(&state)?;
        let envelope = verify_remote_with_key(&remote, &key)?;
        if remote.content_hash == state.last_synced_content_hash {
            return Ok(SyncOperationResult {
                status: "unchanged".into(),
                sync: status_from_state(Some(state)),
            });
        }
        if self.vault.sync_snapshot()?.revision != state.last_synced_vault_revision {
            return Err(SyncServiceError::Conflict);
        }
        let content = decrypt_vault_with_key(&envelope, &key).map_err(map_crypto_error)?;
        self.vault.replace_from_sync(&content)?;
        let state = state_from_remote(
            SyncProvider::parse(&state.provider)?,
            &snapshot_metadata(&envelope),
            &remote,
            state.device_id,
            token.to_owned(),
            state.derived_sync_key,
            state.auto_sync,
        );
        self.state_store.save(&state)?;
        Ok(SyncOperationResult {
            status: "downloaded".into(),
            sync: status_from_state(Some(state)),
        })
    }

    pub async fn resolve_keep_local(
        &self,
        token: &str,
    ) -> Result<SyncOperationResult, SyncServiceError> {
        let state = self.configured_state()?;
        let snapshot = self.vault.sync_snapshot()?;
        let remote_client = self.remote_for_state(&state)?;
        let current = remote_client.get(token, &state.remote_id).await?;
        let key = saved_key(&state)?;
        let envelope = verify_remote_with_key(&current, &key)?;
        self.back_up_conflict(&snapshot.content, &current.content)?;
        let salt = envelope
            .encryption
            .validate()
            .map_err(|_| SyncServiceError::InvalidRemoteData)?
            .0;
        let encrypted = encrypt_snapshot_with_key(snapshot, &key, state.device_id.clone(), salt)?;
        let remote = remote_client
            .update(token, &state.remote_id, &serialize_envelope(&encrypted)?)
            .await?;
        let state = state_from_remote(
            SyncProvider::parse(&state.provider)?,
            &snapshot_metadata(&encrypted),
            &remote,
            state.device_id,
            token.to_owned(),
            state.derived_sync_key,
            state.auto_sync,
        );
        self.state_store.save(&state)?;
        Ok(SyncOperationResult {
            status: "conflict_kept_local".into(),
            sync: status_from_state(Some(state)),
        })
    }

    pub async fn resolve_accept_remote(
        &self,
        token: &str,
    ) -> Result<SyncOperationResult, SyncServiceError> {
        let state = self.configured_state()?;
        let snapshot = self.vault.sync_snapshot()?;
        let remote = self
            .remote_for_state(&state)?
            .get(token, &state.remote_id)
            .await?;
        let key = saved_key(&state)?;
        let envelope = verify_remote_with_key(&remote, &key)?;
        let content = decrypt_vault_with_key(&envelope, &key).map_err(map_crypto_error)?;
        self.back_up_conflict(&snapshot.content, &remote.content)?;
        self.vault.replace_from_sync(&content)?;
        let state = state_from_remote(
            SyncProvider::parse(&state.provider)?,
            &snapshot_metadata(&envelope),
            &remote,
            state.device_id,
            token.to_owned(),
            state.derived_sync_key,
            state.auto_sync,
        );
        self.state_store.save(&state)?;
        Ok(SyncOperationResult {
            status: "conflict_accepted_remote".into(),
            sync: status_from_state(Some(state)),
        })
    }

    pub async fn delete_remote(&self, token: &str) -> Result<(), SyncServiceError> {
        let state = self.configured_state()?;
        self.remote_for_state(&state)?
            .delete(token, &state.remote_id)
            .await?;
        self.state_store.clear()?;
        Ok(())
    }

    fn back_up_conflict(
        &self,
        local_vault: &[u8],
        remote_ciphertext: &str,
    ) -> Result<(), SyncServiceError> {
        let backup_directory = self.app_dir.join("sync-conflicts");
        fs::create_dir_all(&backup_directory)
            .map_err(|error| SyncStateError::Storage(error.to_string()))?;
        let prefix = format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
            uuid::Uuid::new_v4()
        );
        write_backup(
            &backup_directory.join(format!("{prefix}-local-vault.json")),
            local_vault,
        )?;
        write_backup(
            &backup_directory.join(format!("{prefix}-remote-envelope.json")),
            remote_ciphertext.as_bytes(),
        )
    }

    fn configured_state(&self) -> Result<SyncState, SyncServiceError> {
        self.state_store
            .load()?
            .ok_or(SyncServiceError::NotConfigured)
    }

    fn remote_for_state(&self, state: &SyncState) -> Result<Remote, SyncServiceError> {
        self.remote(SyncProvider::parse(&state.provider)?)
    }

    fn remote(&self, provider: SyncProvider) -> Result<Remote, SyncServiceError> {
        match provider {
            SyncProvider::GithubGist => Ok(Remote::Github(GithubGistRemote::new()?)),
            SyncProvider::GiteeSnippet => Ok(Remote::Gitee(GiteeSnippetRemote::new()?)),
        }
    }
}

fn write_backup(path: &Path, content: &[u8]) -> Result<(), SyncServiceError> {
    let temporary = path.with_file_name(format!(".tmp-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), SyncServiceError> {
        let mut file =
            File::create(&temporary).map_err(|error| SyncStateError::Storage(error.to_string()))?;
        file.write_all(content)
            .map_err(|error| SyncStateError::Storage(error.to_string()))?;
        file.sync_all()
            .map_err(|error| SyncStateError::Storage(error.to_string()))?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| SyncStateError::Storage(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn encrypt_snapshot(
    snapshot: VaultSyncSnapshot,
    password: String,
    device_id: String,
) -> Result<EncryptedVault, SyncServiceError> {
    encrypt_vault(
        &snapshot.content,
        password,
        snapshot.vault_id,
        snapshot.revision,
        snapshot.updated_at,
        device_id,
    )
    .map_err(Into::into)
}

fn encrypt_snapshot_with_key(
    snapshot: VaultSyncSnapshot,
    key: &[u8; super::SYNC_KEY_LENGTH],
    device_id: String,
    salt: [u8; super::models::SALT_LENGTH],
) -> Result<EncryptedVault, SyncServiceError> {
    encrypt_vault_with_key(
        &snapshot.content,
        key,
        snapshot.vault_id,
        snapshot.revision,
        snapshot.updated_at,
        device_id,
        salt,
    )
    .map_err(Into::into)
}

fn derive_key_for_envelope(
    envelope: &EncryptedVault,
    password: String,
) -> Result<String, SyncServiceError> {
    let salt = envelope
        .encryption
        .validate()
        .map_err(|_| SyncServiceError::InvalidRemoteData)?
        .0;
    let mut key = derive_sync_key(password, &salt)?;
    let encoded = STANDARD.encode(key);
    key.zeroize();
    Ok(encoded)
}

fn verify_remote_with_key(
    remote: &RemoteDocument,
    key: &[u8; super::SYNC_KEY_LENGTH],
) -> Result<EncryptedVault, SyncServiceError> {
    let envelope = parse_envelope(remote)?;
    decrypt_vault_with_key(&envelope, key).map_err(map_crypto_error)?;
    Ok(envelope)
}

fn saved_key(state: &SyncState) -> Result<[u8; super::SYNC_KEY_LENGTH], SyncServiceError> {
    let bytes = STANDARD
        .decode(&state.derived_sync_key)
        .map_err(|_| SyncServiceError::InvalidRemoteData)?;
    bytes
        .try_into()
        .map_err(|_| SyncServiceError::InvalidRemoteData)
}

fn serialize_envelope(envelope: &EncryptedVault) -> Result<String, SyncServiceError> {
    serde_json::to_string_pretty(envelope).map_err(|_| SyncServiceError::InvalidRemoteData)
}

fn parse_envelope(remote: &RemoteDocument) -> Result<EncryptedVault, SyncServiceError> {
    serde_json::from_str(&remote.content).map_err(|_| SyncServiceError::InvalidRemoteData)
}

fn snapshot_metadata(envelope: &EncryptedVault) -> VaultSyncSnapshot {
    VaultSyncSnapshot {
        content: Vec::new(),
        vault_id: envelope.vault_id.clone(),
        revision: envelope.revision,
        updated_at: envelope.updated_at.clone(),
    }
}

fn state_from_remote(
    provider: SyncProvider,
    snapshot: &VaultSyncSnapshot,
    remote: &RemoteDocument,
    device_id: String,
    token: String,
    derived_sync_key: String,
    auto_sync: bool,
) -> SyncState {
    SyncState {
        provider: provider.as_str().into(),
        remote_id: remote.remote_id.clone(),
        last_synced_content_hash: remote.content_hash.clone(),
        last_synced_vault_revision: snapshot.revision,
        last_synced_at: chrono::Utc::now().to_rfc3339(),
        device_id,
        token,
        derived_sync_key,
        auto_sync,
    }
}

fn status_from_state(state: Option<SyncState>) -> SyncStatus {
    match state {
        Some(state) => SyncStatus {
            configured: true,
            provider: Some(state.provider),
            remote_id: Some(state.remote_id),
            remote_file_name: Some(GIST_FILE_NAME.into()),
            state: "idle".into(),
            last_synced_at: Some(state.last_synced_at),
            device_id: Some(state.device_id),
            token: Some(state.token),
            auto_sync: state.auto_sync,
            local_vault_revision: None,
            last_synced_vault_revision: Some(state.last_synced_vault_revision),
        },
        None => SyncStatus {
            configured: false,
            provider: None,
            remote_id: None,
            remote_file_name: None,
            state: "disabled".into(),
            last_synced_at: None,
            device_id: None,
            token: None,
            auto_sync: false,
            local_vault_revision: None,
            last_synced_vault_revision: None,
        },
    }
}

fn map_crypto_error(error: SyncCryptoError) -> SyncServiceError {
    match error {
        SyncCryptoError::Decryption | SyncCryptoError::InvalidEnvelope(_) => {
            SyncServiceError::InvalidRemoteData
        }
        error => SyncServiceError::Crypto(error),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn remote_key_verification_reports_invalid_remote_data() {
        let envelope = encrypt_vault(
            br#"{\"formatVersion\":1,\"profiles\":[]}"#,
            "correct sync password".into(),
            uuid::Uuid::new_v4().to_string(),
            1,
            "2026-07-21T00:00:00Z".into(),
            uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        let remote = RemoteDocument {
            remote_id: "remote".into(),
            content: serde_json::to_string(&envelope).unwrap(),
            content_hash: "sha256:test".into(),
        };

        let error =
            verify_remote_with_key(&remote, &[0; crate::sync::SYNC_KEY_LENGTH]).unwrap_err();

        assert!(matches!(error, SyncServiceError::InvalidRemoteData));
    }

    #[test]
    fn conflict_backup_preserves_local_vault_and_remote_envelope() {
        let app_dir =
            std::env::temp_dir().join(format!("mjjssh-sync-backup-{}", uuid::Uuid::new_v4()));
        let vault = Vault::open(&app_dir).unwrap();
        let service = SyncService::new(&vault, &app_dir).unwrap();

        service
            .back_up_conflict(b"{\"local\":true}", "{\"ciphertext\":true}")
            .unwrap();

        let backup_directory = app_dir.join("sync-conflicts");
        let entries = fs::read_dir(&backup_directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        let local_backup = entries
            .iter()
            .find(|path| path.to_string_lossy().ends_with("-local-vault.json"))
            .unwrap();
        let remote_backup = entries
            .iter()
            .find(|path| path.to_string_lossy().ends_with("-remote-envelope.json"))
            .unwrap();
        assert_eq!(fs::read(local_backup).unwrap(), b"{\"local\":true}");
        assert_eq!(fs::read(remote_backup).unwrap(), b"{\"ciphertext\":true}");

        fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn conflict_backup_stops_when_backup_directory_is_a_file() {
        let app_dir =
            std::env::temp_dir().join(format!("mjjssh-sync-backup-{}", uuid::Uuid::new_v4()));
        let vault = Vault::open(&app_dir).unwrap();
        let service = SyncService::new(&vault, &app_dir).unwrap();
        fs::write(app_dir.join("sync-conflicts"), b"not a directory").unwrap();

        assert!(service.back_up_conflict(b"local", "remote").is_err());
        assert!(!app_dir.join("sync-conflicts-local-vault.json").exists());

        fs::remove_dir_all(app_dir).unwrap();
    }
}
