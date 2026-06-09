use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

const SAFETY_OFFSET_MS: i64 = 5 * 60 * 1000;
const SERVICE: &str = "vibes-left-token-cache";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCredentials {
    pub token: String,
    pub expires_at_ms: i64,
}

/// Keyring-backed token cache, one instance per provider.
/// Each provider gets its own keychain entry (`SERVICE / provider`),
/// so reads never cross app-ownership boundaries and macOS won't prompt.
pub struct CredsCache {
    provider: &'static str,
}

impl CredsCache {
    pub fn new(provider: &'static str) -> Self {
        Self { provider }
    }

    fn entry(&self) -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(SERVICE, self.provider)
            .map_err(|e| AppError::Internal(format!("keyring entry: {e}")))
    }

    pub fn get(&self) -> Option<CachedCredentials> {
        let json = self.entry().ok()?.get_password().ok()?;
        let creds: CachedCredentials = serde_json::from_str(&json).ok()?;
        if now_ms() < creds.expires_at_ms - SAFETY_OFFSET_MS {
            Some(creds)
        } else {
            None
        }
    }

    pub fn put(&self, creds: &CachedCredentials) -> Result<(), AppError> {
        let json = serde_json::to_string(creds)?;
        self.entry()?
            .set_password(&json)
            .map_err(|e| AppError::Internal(format!("keyring write: {e}")))
    }

    pub fn clear(&self) -> Result<(), AppError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Internal(format!("keyring delete: {e}"))),
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
