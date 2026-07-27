use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::local_security::{
    create_local_vault_key, decrypt_known_hosts, encrypt_known_hosts, existing_local_vault_key,
    LocalEncryptedVault, LocalSecurityError, LOCAL_KEY_LENGTH,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrustedHostKey {
    pub algorithm: String,
    pub fingerprint: String,
    pub trusted_at: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct KnownHostsDocument {
    #[serde(default)]
    hosts: HashMap<String, TrustedHostKey>,
}

pub struct KnownHosts {
    path: PathBuf,
    local_key: [u8; LOCAL_KEY_LENGTH],
    hosts: HashMap<String, TrustedHostKey>,
}

#[derive(Debug, thiserror::Error)]
pub enum KnownHostsError {
    #[error("system credential storage is unavailable: {0}")]
    CredentialStore(#[from] LocalSecurityError),
    #[error("known host encryption key is unavailable in the system credential store")]
    LocalKeyUnavailable,
    #[error("known hosts file is invalid or could not be decrypted")]
    InvalidFormat,
    #[error("known hosts storage error: {0}")]
    Storage(#[from] std::io::Error),
}

impl KnownHosts {
    pub fn open(app_dir: PathBuf) -> Result<Self, KnownHostsError> {
        fs::create_dir_all(&app_dir)?;
        let path = app_dir.join("known_hosts.json");
        let local_key = if path.exists() {
            existing_local_vault_key()?.ok_or(KnownHostsError::LocalKeyUnavailable)?
        } else {
            create_local_vault_key()?
        };
        Self::open_with_key(path, local_key)
    }

    fn open_with_key(
        path: PathBuf,
        local_key: [u8; LOCAL_KEY_LENGTH],
    ) -> Result<Self, KnownHostsError> {
        let hosts = match fs::read(&path) {
            Ok(contents) => {
                let envelope: LocalEncryptedVault = serde_json::from_slice(&contents)
                    .map_err(|_| KnownHostsError::InvalidFormat)?;
                let plaintext = decrypt_known_hosts(&envelope, &local_key)
                    .map_err(|_| KnownHostsError::InvalidFormat)?;
                serde_json::from_slice::<KnownHostsDocument>(&plaintext)
                    .map(|document| document.hosts)
                    .map_err(|_| KnownHostsError::InvalidFormat)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            local_key,
            hosts,
        })
    }

    #[cfg(test)]
    fn open_for_test(path: PathBuf) -> Result<Self, KnownHostsError> {
        Self::open_with_key(path, [7_u8; LOCAL_KEY_LENGTH])
    }

    pub fn host_id(host: &str, port: u16) -> String {
        format!("{}:{}", host.trim().to_ascii_lowercase(), port)
    }

    pub fn get(&self, host: &str, port: u16) -> Option<&TrustedHostKey> {
        self.hosts.get(&Self::host_id(host, port))
    }

    pub fn trust(
        &mut self,
        host: &str,
        port: u16,
        algorithm: String,
        fingerprint: String,
    ) -> Result<(), KnownHostsError> {
        self.hosts.insert(
            Self::host_id(host, port),
            TrustedHostKey {
                algorithm,
                fingerprint,
                trusted_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        self.save()
    }

    fn save(&self) -> Result<(), KnownHostsError> {
        let document = serde_json::to_vec(&KnownHostsDocument {
            hosts: self.hosts.clone(),
        })
        .map_err(std::io::Error::other)?;
        let envelope = encrypt_known_hosts(&document, &self.local_key)?;
        let contents = serde_json::to_vec_pretty(&envelope).map_err(std::io::Error::other)?;
        let temporary_path = self.path.with_extension("json.tmp");
        let mut file = fs::File::create(&temporary_path)?;
        file.write_all(&contents)?;
        file.sync_all()?;
        fs::rename(temporary_path, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{KnownHosts, KnownHostsError};

    fn temporary_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mjjssh-known-hosts-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn normalizes_host_identity() {
        assert_eq!(KnownHosts::host_id(" Example.COM ", 22), "example.com:22");
    }

    #[test]
    fn persists_encrypted_known_hosts() {
        let directory = temporary_path();
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("known_hosts.json");
        let mut known_hosts = KnownHosts::open_for_test(path.clone()).unwrap();
        known_hosts
            .trust(
                "120.24.111.246",
                22,
                "ssh-ed25519".into(),
                "SHA256:example".into(),
            )
            .unwrap();

        let encrypted = fs::read_to_string(&path).unwrap();
        assert!(!encrypted.contains("120.24.111.246"));
        assert!(!encrypted.contains("SHA256:example"));

        let reopened = KnownHosts::open_for_test(path).unwrap();
        assert_eq!(
            reopened.get("120.24.111.246", 22).unwrap().fingerprint,
            "SHA256:example"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_plaintext_or_tampered_known_hosts() {
        let directory = temporary_path();
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("known_hosts.json");
        fs::write(&path, r#"{\"hosts\":{}}"#).unwrap();
        assert!(matches!(
            KnownHosts::open_for_test(path.clone()),
            Err(KnownHostsError::InvalidFormat)
        ));

        fs::write(
            &path,
            r#"{\"formatVersion\":1,\"cipher\":\"aes-256-gcm\",\"nonce\":\"AAAAAAAAAAAAAAAA\",\"ciphertext\":\"AAAAAAAAAAAAAAAAAAAAAA==\"}"#,
        )
        .unwrap();
        assert!(matches!(
            KnownHosts::open_for_test(path),
            Err(KnownHostsError::InvalidFormat)
        ));
        fs::remove_dir_all(directory).unwrap();
    }
}
