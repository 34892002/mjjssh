use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::local_security::{
    decrypt_command_history, encrypt_command_history, LocalEncryptedVault, LocalSecurityError,
    LOCAL_KEY_LENGTH,
};

const HISTORY_FILE_NAME: &str = "command-history.json";
const COMMAND_HISTORY_LIMIT: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum CommandHistoryError {
    #[error("command history storage error: {0}")]
    Storage(String),
    #[error("command history file is invalid: {0}")]
    InvalidFormat(String),
    #[error("command history encryption error: {0}")]
    LocalSecurity(#[from] LocalSecurityError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandHistoryDocument {
    #[serde(default)]
    entries: Vec<String>,
}

/// Command history is intentionally local-only and encrypted with the same
/// local Vault key. In particular, recording a command must not mutate the
/// Vault document, otherwise every terminal input would bump the sync revision
/// and create needless cloud sync conflicts.
pub struct CommandHistoryStore {
    path: PathBuf,
    key: [u8; LOCAL_KEY_LENGTH],
}

impl CommandHistoryStore {
    pub fn new(app_dir: &Path, key: [u8; LOCAL_KEY_LENGTH]) -> Self {
        Self {
            path: app_dir.join(HISTORY_FILE_NAME),
            key,
        }
    }

    pub fn load(&self) -> Result<Vec<String>, CommandHistoryError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.path).map_err(io_error)?;
        let envelope: LocalEncryptedVault = serde_json::from_str(&content)
            .map_err(|error| CommandHistoryError::InvalidFormat(error.to_string()))?;
        let plaintext = decrypt_command_history(&envelope, &self.key)?;
        let document: CommandHistoryDocument = serde_json::from_slice(&plaintext)
            .map_err(|error| CommandHistoryError::InvalidFormat(error.to_string()))?;
        Ok(document.entries)
    }

    pub fn record(&self, command: &str) -> Result<Vec<String>, CommandHistoryError> {
        let mut entries = self.load()?;
        let command = command.trim();
        if command.is_empty() {
            return Ok(entries);
        }
        entries.retain(|entry| entry != command);
        entries.insert(0, command.to_owned());
        entries.truncate(COMMAND_HISTORY_LIMIT);
        self.save(&entries)?;
        Ok(entries)
    }

    pub fn clear(&self) -> Result<(), CommandHistoryError> {
        self.save(&[])
    }

    /// Migrate history from older Vault documents that still embedded it.
    /// An existing local history file always wins.
    pub fn import_legacy(&self, legacy_entries: &[String]) -> Result<(), CommandHistoryError> {
        if legacy_entries.is_empty() {
            return Ok(());
        }
        if self.path.exists() {
            match self.load() {
                Ok(_) => return Ok(()),
                Err(CommandHistoryError::InvalidFormat(_)) => {
                    // Recover an unreadable history file from the legacy Vault data.
                }
                Err(error) => return Err(error),
            }
        }

        let mut entries = Vec::new();
        for entry in legacy_entries {
            let entry = entry.trim();
            if entry.is_empty() || entries.iter().any(|existing| existing == entry) {
                continue;
            }
            entries.push(entry.to_owned());
            if entries.len() == COMMAND_HISTORY_LIMIT {
                break;
            }
        }
        self.save(&entries)
    }

    fn save(&self, entries: &[String]) -> Result<(), CommandHistoryError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let plaintext = serde_json::to_vec(&CommandHistoryDocument {
            entries: entries.to_vec(),
        })
        .map_err(|error| CommandHistoryError::Storage(error.to_string()))?;
        let envelope = encrypt_command_history(&plaintext, &self.key)?;
        let content = serde_json::to_vec_pretty(&envelope)
            .map_err(|error| CommandHistoryError::Storage(error.to_string()))?;
        let temporary = self.path.with_file_name(format!(
            ".command-history.json.tmp-{}",
            uuid::Uuid::new_v4()
        ));
        let result = (|| -> Result<(), CommandHistoryError> {
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
}

fn io_error(error: std::io::Error) -> CommandHistoryError {
    CommandHistoryError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("mjjssh-history-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("create temporary history directory");
        directory
    }

    fn test_store(directory: &Path) -> CommandHistoryStore {
        CommandHistoryStore::new(directory, [7_u8; LOCAL_KEY_LENGTH])
    }

    #[test]
    fn records_deduplicates_and_persists_history() {
        let directory = test_dir();
        let store = test_store(&directory);

        store.record("ls -la").unwrap();
        store.record("cd /tmp").unwrap();
        let entries = store.record("ls -la").unwrap();

        assert_eq!(entries, vec!["ls -la", "cd /tmp"]);
        assert_eq!(store.load().unwrap(), entries);

        let persisted = fs::read_to_string(directory.join(HISTORY_FILE_NAME)).unwrap();
        assert!(!persisted.contains("ls -la"));
        assert!(persisted.contains("ciphertext"));

        fs::remove_dir_all(directory).expect("remove temporary history directory");
    }

    #[test]
    fn clearing_history_persists_an_empty_list() {
        let directory = test_dir();
        let store = test_store(&directory);
        store.record("uptime").unwrap();

        store.clear().unwrap();

        assert!(store.load().unwrap().is_empty());
        fs::remove_dir_all(directory).expect("remove temporary history directory");
    }

    #[test]
    fn imports_legacy_history_only_once() {
        let directory = test_dir();
        let store = test_store(&directory);

        store
            .import_legacy(&["ls".into(), "ls".into(), " whoami ".into()])
            .unwrap();
        assert_eq!(store.load().unwrap(), vec!["ls", "whoami"]);

        store.import_legacy(&["ignored".into()]).unwrap();
        assert_eq!(store.load().unwrap(), vec!["ls", "whoami"]);

        fs::remove_dir_all(directory).expect("remove temporary history directory");
    }
}
