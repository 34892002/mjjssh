use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

use super::crypto::SYNC_KEY_LENGTH;

const STATE_FILE_NAME: &str = "sync.json";

#[derive(Debug, thiserror::Error)]
pub enum SyncStateError {
    #[error("sync state storage error: {0}")]
    Storage(String),
    #[error("sync state is invalid: {0}")]
    Invalid(String),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub provider: String,
    pub remote_id: String,
    pub last_synced_content_hash: String,
    pub last_synced_vault_revision: u64,
    pub last_synced_at: String,
    pub device_id: String,
    pub token: String,
    pub derived_sync_key: String,
    #[serde(default = "default_auto_sync")]
    pub auto_sync: bool,
}

const fn default_auto_sync() -> bool {
    true
}

pub struct SyncStateStore {
    path: PathBuf,
}

impl SyncStateStore {
    pub fn new(app_dir: &Path) -> Self {
        Self {
            path: app_dir.join(STATE_FILE_NAME),
        }
    }

    pub fn load(&self) -> Result<Option<SyncState>, SyncStateError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&self.path).map_err(io_error)?;
        let state: SyncState = serde_json::from_str(&content)
            .map_err(|error| SyncStateError::Invalid(error.to_string()))?;
        validate(&state)?;
        Ok(Some(state))
    }

    pub fn save(&self, state: &SyncState) -> Result<(), SyncStateError> {
        validate(state)?;
        let content = serde_json::to_vec_pretty(state)
            .map_err(|error| SyncStateError::Storage(error.to_string()))?;
        let temporary = self
            .path
            .with_file_name(format!(".sync.json.tmp-{}", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), SyncStateError> {
            let mut file = File::create(&temporary).map_err(io_error)?;
            file.write_all(&content).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            drop(file);
            fs::rename(&temporary, &self.path).map_err(io_error)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn clear(&self) -> Result<(), SyncStateError> {
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(io_error)?;
        }
        Ok(())
    }
}

fn validate(state: &SyncState) -> Result<(), SyncStateError> {
    if !matches!(state.provider.as_str(), "github_gist" | "gitee_snippet") {
        return Err(SyncStateError::Invalid("unsupported provider".into()));
    }
    if state.remote_id.trim().is_empty()
        || state.last_synced_content_hash.trim().is_empty()
        || state.token.trim().is_empty()
        || uuid::Uuid::parse_str(&state.device_id).is_err()
    {
        return Err(SyncStateError::Invalid(
            "required metadata is invalid".into(),
        ));
    }
    let derived_key = STANDARD
        .decode(&state.derived_sync_key)
        .map_err(|_| SyncStateError::Invalid("derived sync key is not Base64".into()))?;
    if derived_key.len() != SYNC_KEY_LENGTH {
        return Err(SyncStateError::Invalid(
            "derived sync key must be 32 bytes".into(),
        ));
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> SyncStateError {
    SyncStateError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> SyncState {
        SyncState {
            provider: "github_gist".into(),
            remote_id: "remote".into(),
            last_synced_content_hash: "hash".into(),
            last_synced_vault_revision: 1,
            last_synced_at: "2026-07-21T00:00:00Z".into(),
            device_id: uuid::Uuid::new_v4().to_string(),
            token: "token".into(),
            derived_sync_key: STANDARD.encode([0u8; SYNC_KEY_LENGTH]),
            auto_sync: true,
        }
    }

    #[test]
    fn saves_and_loads_sync_credentials() {
        let directory =
            std::env::temp_dir().join(format!("mjjssh-sync-state-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("create temporary sync state directory");
        let store = SyncStateStore::new(&directory);
        let state = sample_state();

        store.save(&state).expect("save sync state");
        let loaded = store
            .load()
            .expect("load sync state")
            .expect("saved sync state should exist");

        assert_eq!(loaded.token, "token");
        assert_eq!(
            loaded.derived_sync_key,
            STANDARD.encode([0u8; SYNC_KEY_LENGTH])
        );
        let persisted =
            fs::read_to_string(directory.join(STATE_FILE_NAME)).expect("read sync state");
        assert!(!persisted.contains("syncPassword"));

        fs::remove_dir_all(directory).expect("remove temporary sync state directory");
    }

    #[test]
    fn rejects_invalid_derived_sync_key() {
        let mut state = sample_state();
        state.derived_sync_key = "invalid".into();

        assert!(matches!(validate(&state), Err(SyncStateError::Invalid(_))));

        state.derived_sync_key = STANDARD.encode([0u8; SYNC_KEY_LENGTH - 1]);
        assert!(matches!(validate(&state), Err(SyncStateError::Invalid(_))));
    }
}
