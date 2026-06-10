use serde::{Deserialize, Serialize};

use crate::error::AppError;

const SERVICE: &str = "vibes-left-token-cache";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCredentials {
    pub token: String,
    pub expires_at_ms: i64,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub account_id: String,
}

/// Token cache backed by the OS keychain in release builds.
/// In debug builds, falls back to a temp file so macOS doesn't prompt on
/// every recompile (unsigned debug binaries trigger the ACL check each time).
pub struct CredsCache {
    provider: &'static str,
}

impl CredsCache {
    pub fn new(provider: &'static str) -> Self {
        Self { provider }
    }

    pub fn get_raw(&self) -> Option<CachedCredentials> {
        let json = self.read_raw().ok()??;
        serde_json::from_str(&json).ok()
    }

    pub fn put(&self, creds: &CachedCredentials) -> Result<(), AppError> {
        let json = serde_json::to_string(creds)?;
        self.write_raw(&json)
    }

    pub fn clear(&self) -> Result<(), AppError> {
        self.delete_raw()
    }

    // --- backend dispatch ---

    fn read_raw(&self) -> Result<Option<String>, AppError> {
        #[cfg(debug_assertions)]
        {
            let path = self.dev_path();
            match std::fs::read_to_string(&path) {
                Ok(s) => Ok(Some(s)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(AppError::Internal(format!("dev cache read: {e}"))),
            }
        }
        #[cfg(not(debug_assertions))]
        {
            match self.entry()?.get_password() {
                Ok(s) => Ok(Some(s)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(AppError::Internal(format!("keyring read: {e}"))),
            }
        }
    }

    fn write_raw(&self, json: &str) -> Result<(), AppError> {
        #[cfg(debug_assertions)]
        {
            std::fs::write(self.dev_path(), json)
                .map_err(|e| AppError::Internal(format!("dev cache write: {e}")))
        }
        #[cfg(not(debug_assertions))]
        {
            self.entry()?
                .set_password(json)
                .map_err(|e| AppError::Internal(format!("keyring write: {e}")))
        }
    }

    fn delete_raw(&self) -> Result<(), AppError> {
        #[cfg(debug_assertions)]
        {
            match std::fs::remove_file(self.dev_path()) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(AppError::Internal(format!("dev cache delete: {e}"))),
            }
        }
        #[cfg(not(debug_assertions))]
        {
            match self.entry()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(AppError::Internal(format!("keyring delete: {e}"))),
            }
        }
    }

    #[cfg(not(debug_assertions))]
    fn entry(&self) -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(SERVICE, self.provider)
            .map_err(|e| AppError::Internal(format!("keyring entry: {e}")))
    }

    /// Returns e.g. `$TMPDIR/vibes-left-token-cache-claude.json`
    #[cfg(debug_assertions)]
    fn dev_path(&self) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("{SERVICE}-{}.json", self.provider))
    }
}
