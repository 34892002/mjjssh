use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const REMOTE_FORMAT_VERSION: u32 = 1;
pub const ARGON2ID_MEMORY_KIB: u32 = 65_536;
pub const ARGON2ID_ITERATIONS: u32 = 3;
pub const ARGON2ID_PARALLELISM: u32 = 4;
pub const SALT_LENGTH: usize = 16;
pub const NONCE_LENGTH: usize = 12;
pub const MAX_CIPHERTEXT_LENGTH: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RemoteDocument {
    pub remote_id: String,
    pub content: String,
    pub content_hash: String,
}

pub fn content_hash(content: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
}

#[derive(Debug, thiserror::Error)]
pub enum SyncEnvelopeError {
    #[error("unsupported remote formatVersion: {0}")]
    UnsupportedFormatVersion(u32),
    #[error("remote Vault ID must be a UUID")]
    InvalidVaultId,
    #[error("remote encryption metadata is invalid: {0}")]
    InvalidEncryptionMetadata(String),
    #[error("remote ciphertext is invalid: {0}")]
    InvalidCiphertext(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KdfParameters {
    pub kdf: String,
    pub kdf_version: u32,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub salt: String,
    pub cipher: String,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionMetadata {
    pub kdf: String,
    pub kdf_version: u32,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub salt: String,
    pub cipher: String,
    pub nonce: String,
}

impl EncryptionMetadata {
    pub fn new(salt: [u8; SALT_LENGTH], nonce: [u8; NONCE_LENGTH]) -> Self {
        Self {
            kdf: "argon2id".into(),
            kdf_version: 1,
            memory_kib: ARGON2ID_MEMORY_KIB,
            iterations: ARGON2ID_ITERATIONS,
            parallelism: ARGON2ID_PARALLELISM,
            salt: STANDARD.encode(salt),
            cipher: "aes-256-gcm".into(),
            nonce: STANDARD.encode(nonce),
        }
    }

    pub fn validate(&self) -> Result<([u8; SALT_LENGTH], [u8; NONCE_LENGTH]), SyncEnvelopeError> {
        if self.kdf != "argon2id"
            || self.kdf_version != 1
            || self.memory_kib != ARGON2ID_MEMORY_KIB
            || self.iterations != ARGON2ID_ITERATIONS
            || self.parallelism != ARGON2ID_PARALLELISM
            || self.cipher != "aes-256-gcm"
        {
            return Err(SyncEnvelopeError::InvalidEncryptionMetadata(
                "unsupported KDF or cipher parameters".into(),
            ));
        }
        let salt = decode_fixed(&self.salt, SALT_LENGTH, "salt")?;
        let nonce = decode_fixed(&self.nonce, NONCE_LENGTH, "nonce")?;
        Ok((salt, nonce))
    }

    pub fn aad_parameters(&self) -> KdfParameters {
        KdfParameters {
            kdf: self.kdf.clone(),
            kdf_version: self.kdf_version,
            memory_kib: self.memory_kib,
            iterations: self.iterations,
            parallelism: self.parallelism,
            salt: self.salt.clone(),
            cipher: self.cipher.clone(),
            nonce: self.nonce.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedVault {
    pub format_version: u32,
    pub vault_id: String,
    pub revision: u64,
    pub updated_at: String,
    pub updated_by_device_id: String,
    pub encryption: EncryptionMetadata,
    pub ciphertext: String,
}

impl EncryptedVault {
    pub fn validate(&self) -> Result<(), SyncEnvelopeError> {
        if self.format_version != REMOTE_FORMAT_VERSION {
            return Err(SyncEnvelopeError::UnsupportedFormatVersion(
                self.format_version,
            ));
        }
        if uuid::Uuid::parse_str(&self.vault_id).is_err() {
            return Err(SyncEnvelopeError::InvalidVaultId);
        }
        self.encryption.validate()?;
        let ciphertext = STANDARD
            .decode(&self.ciphertext)
            .map_err(|_| SyncEnvelopeError::InvalidCiphertext("not Base64".into()))?;
        if ciphertext.len() < 16 || ciphertext.len() > MAX_CIPHERTEXT_LENGTH {
            return Err(SyncEnvelopeError::InvalidCiphertext(
                "size is outside the accepted range".into(),
            ));
        }
        Ok(())
    }
}

fn decode_fixed<const N: usize>(
    value: &str,
    expected_length: usize,
    field: &str,
) -> Result<[u8; N], SyncEnvelopeError> {
    let bytes = STANDARD.decode(value).map_err(|_| {
        SyncEnvelopeError::InvalidEncryptionMetadata(format!("{field} is not Base64"))
    })?;
    if bytes.len() != expected_length {
        return Err(SyncEnvelopeError::InvalidEncryptionMetadata(format!(
            "{field} has an invalid length"
        )));
    }
    bytes.try_into().map_err(|_| {
        SyncEnvelopeError::InvalidEncryptionMetadata(format!("{field} has an invalid length"))
    })
}
