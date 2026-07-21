mod crypto;
pub mod gitee_snippet;
pub mod github_gist;
mod models;
pub mod service;
pub mod state;

pub use crypto::{
    decrypt_vault, decrypt_vault_with_key, derive_sync_key, encrypt_vault, encrypt_vault_with_key,
    SyncCryptoError, SYNC_KEY_LENGTH,
};
pub use models::{EncryptedVault, EncryptionMetadata, KdfParameters, SyncEnvelopeError};
