use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const LOCAL_KEY_LENGTH: usize = 32;
const LOCAL_NONCE_LENGTH: usize = 12;
const LOCAL_FORMAT_VERSION: u32 = 1;
const LOCAL_VAULT_AAD: &[u8] = b"mjjssh-local-vault-v1";
const LOCAL_KNOWN_HOSTS_AAD: &[u8] = b"mjjssh-local-known-hosts-v1";
const SERVICE_NAME: &str = "com.mjjssh.app";
const VAULT_KEY_ACCOUNT: &str = "local-vault-key-v1";

#[derive(Debug, thiserror::Error)]
pub enum LocalSecurityError {
    #[error("system credential storage is unavailable: {0}")]
    CredentialStore(String),
    #[error("local encrypted configuration is invalid")]
    InvalidEnvelope,
    #[error("local configuration could not be decrypted")]
    Decryption,
    #[error("local configuration could not be encrypted")]
    Encryption,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEncryptedVault {
    pub format_version: u32,
    pub cipher: String,
    pub nonce: String,
    pub ciphertext: String,
}

pub fn existing_local_vault_key() -> Result<Option<[u8; LOCAL_KEY_LENGTH]>, LocalSecurityError> {
    credential_get(VAULT_KEY_ACCOUNT)?.map_or(Ok(None), |value| decode_key(&value).map(Some))
}

pub fn create_local_vault_key() -> Result<[u8; LOCAL_KEY_LENGTH], LocalSecurityError> {
    if let Some(key) = existing_local_vault_key()? {
        return Ok(key);
    }

    let mut key = [0_u8; LOCAL_KEY_LENGTH];
    OsRng.fill_bytes(&mut key);
    let encoded = STANDARD.encode(key);
    credential_set(VAULT_KEY_ACCOUNT, &encoded)?;
    Ok(key)
}

pub fn sync_secret_account(secret_name: &str) -> String {
    format!("sync-v1:{secret_name}")
}

pub fn get_sync_secret(secret_name: &str) -> Result<Option<String>, LocalSecurityError> {
    credential_get(&sync_secret_account(secret_name))
}

pub fn set_sync_secret(secret_name: &str, value: &str) -> Result<(), LocalSecurityError> {
    credential_set(&sync_secret_account(secret_name), value)
}

pub fn delete_sync_secret(secret_name: &str) -> Result<(), LocalSecurityError> {
    credential_delete(&sync_secret_account(secret_name))
}

pub fn encrypt_vault(
    plaintext: &[u8],
    key: &[u8; LOCAL_KEY_LENGTH],
) -> Result<LocalEncryptedVault, LocalSecurityError> {
    encrypt_local_data(plaintext, key, LOCAL_VAULT_AAD)
}

pub fn encrypt_known_hosts(
    plaintext: &[u8],
    key: &[u8; LOCAL_KEY_LENGTH],
) -> Result<LocalEncryptedVault, LocalSecurityError> {
    encrypt_local_data(plaintext, key, LOCAL_KNOWN_HOSTS_AAD)
}

fn encrypt_local_data(
    plaintext: &[u8],
    key: &[u8; LOCAL_KEY_LENGTH],
    aad: &[u8],
) -> Result<LocalEncryptedVault, LocalSecurityError> {
    let mut nonce = [0_u8; LOCAL_NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| LocalSecurityError::Encryption)?;
    let nonce_ref =
        Nonce::try_from(nonce.as_slice()).map_err(|_| LocalSecurityError::Encryption)?;
    let ciphertext = cipher
        .encrypt(
            &nonce_ref,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| LocalSecurityError::Encryption)?;
    Ok(LocalEncryptedVault {
        format_version: LOCAL_FORMAT_VERSION,
        cipher: "aes-256-gcm".into(),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    })
}

pub fn decrypt_vault(
    envelope: &LocalEncryptedVault,
    key: &[u8; LOCAL_KEY_LENGTH],
) -> Result<Vec<u8>, LocalSecurityError> {
    decrypt_local_data(envelope, key, LOCAL_VAULT_AAD)
}

pub fn decrypt_known_hosts(
    envelope: &LocalEncryptedVault,
    key: &[u8; LOCAL_KEY_LENGTH],
) -> Result<Vec<u8>, LocalSecurityError> {
    decrypt_local_data(envelope, key, LOCAL_KNOWN_HOSTS_AAD)
}

fn decrypt_local_data(
    envelope: &LocalEncryptedVault,
    key: &[u8; LOCAL_KEY_LENGTH],
    aad: &[u8],
) -> Result<Vec<u8>, LocalSecurityError> {
    if envelope.format_version != LOCAL_FORMAT_VERSION || envelope.cipher != "aes-256-gcm" {
        return Err(LocalSecurityError::InvalidEnvelope);
    }
    let nonce = STANDARD
        .decode(&envelope.nonce)
        .map_err(|_| LocalSecurityError::InvalidEnvelope)?;
    let nonce: [u8; LOCAL_NONCE_LENGTH] = nonce
        .try_into()
        .map_err(|_| LocalSecurityError::InvalidEnvelope)?;
    let ciphertext = STANDARD
        .decode(&envelope.ciphertext)
        .map_err(|_| LocalSecurityError::InvalidEnvelope)?;
    if ciphertext.len() < 16 {
        return Err(LocalSecurityError::InvalidEnvelope);
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| LocalSecurityError::Decryption)?;
    let nonce_ref =
        Nonce::try_from(nonce.as_slice()).map_err(|_| LocalSecurityError::InvalidEnvelope)?;
    cipher
        .decrypt(
            &nonce_ref,
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .map_err(|_| LocalSecurityError::Decryption)
}

fn credential_entry(account: &str) -> Result<keyring::Entry, LocalSecurityError> {
    let target = format!("{SERVICE_NAME}.{account}");
    keyring::Entry::new_with_target(&target, SERVICE_NAME, account)
        .map_err(|error| LocalSecurityError::CredentialStore(error.to_string()))
}

fn credential_get(account: &str) -> Result<Option<String>, LocalSecurityError> {
    let entry = credential_entry(account)?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(LocalSecurityError::CredentialStore(error.to_string())),
    }
}

fn credential_set(account: &str, value: &str) -> Result<(), LocalSecurityError> {
    credential_entry(account)?
        .set_password(value)
        .map_err(|error| LocalSecurityError::CredentialStore(error.to_string()))
}

fn credential_delete(account: &str) -> Result<(), LocalSecurityError> {
    let entry = credential_entry(account)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(LocalSecurityError::CredentialStore(error.to_string())),
    }
}

fn decode_key(value: &str) -> Result<[u8; LOCAL_KEY_LENGTH], LocalSecurityError> {
    let mut bytes = STANDARD
        .decode(value)
        .map_err(|_| LocalSecurityError::CredentialStore("stored local key is invalid".into()))?;
    let result = bytes.as_slice().try_into().map_err(|_| {
        LocalSecurityError::CredentialStore("stored local key has invalid length".into())
    });
    bytes.zeroize();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_vault_round_trips_and_rejects_tampering() {
        let key = [7_u8; LOCAL_KEY_LENGTH];
        let envelope = encrypt_vault(b"secret vault", &key).unwrap();
        assert_eq!(decrypt_vault(&envelope, &key).unwrap(), b"secret vault");

        let mut changed = envelope;
        let mut ciphertext = STANDARD.decode(&changed.ciphertext).unwrap();
        ciphertext[0] ^= 1;
        changed.ciphertext = STANDARD.encode(ciphertext);
        assert!(decrypt_vault(&changed, &key).is_err());
    }

    #[test]
    fn sync_secret_accounts_are_fixed_for_the_single_sync_configuration() {
        assert_eq!(sync_secret_account("token"), "sync-v1:token");
        assert_eq!(sync_secret_account("derived-key"), "sync-v1:derived-key");
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn system_credential_store_round_trips_an_encryption_key() {
        struct CredentialCleanup(String);

        impl Drop for CredentialCleanup {
            fn drop(&mut self) {
                let _ = credential_delete(&self.0);
            }
        }

        let account = format!("test-v1:{}", uuid::Uuid::new_v4());
        let _cleanup = CredentialCleanup(account.clone());
        let mut key = [0_u8; LOCAL_KEY_LENGTH];
        OsRng.fill_bytes(&mut key);
        let encoded_key = STANDARD.encode(key);

        credential_set(&account, &encoded_key).unwrap();
        let stored_key = credential_get(&account).unwrap().unwrap();
        let recovered_key = decode_key(&stored_key).unwrap();
        let envelope = encrypt_vault(b"system credential store test", &recovered_key).unwrap();

        assert_eq!(
            decrypt_vault(&envelope, &recovered_key).unwrap(),
            b"system credential store test"
        );
    }
}
