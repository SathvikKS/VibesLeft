use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::{CachedCredentials, CredsCache};
use crate::error::AppError;

const PROACTIVE_REFRESH_MS: i64 = 15 * 60 * 1_000;

#[async_trait]
pub(super) trait CredentialSource: Send + Sync {
    /// Cache key / provider tag, e.g. "claude", "antigravity".
    fn provider(&self) -> &'static str;

    /// Read creds from the ORIGINAL OS keychain / on-disk file.
    /// Bootstrap + last-resort only; never written back to.
    fn fetch_from_source(&self) -> Result<CachedCredentials, AppError>;

    /// Whether this provider can exchange a refresh token.
    fn supports_refresh(&self) -> bool;

    /// Exchange a refresh token for fresh creds.
    /// MUST return AppError::ReauthRequired on a 400/401 from the token endpoint
    /// (refresh token is dead). Network/5xx => AppError::Internal.
    async fn refresh(&self, _refresh_token: &str) -> Result<CachedCredentials, AppError> {
        Err(AppError::Internal("refresh not supported".into()))
    }
}

pub(super) struct TokenManager<S: CredentialSource> {
    cache: CredsCache,
    source: S,
    lock: Mutex<()>,
}

impl<S: CredentialSource> TokenManager<S> {
    pub fn new(source: S) -> Self {
        Self {
            cache: CredsCache::new(source.provider()),
            source,
            lock: Mutex::new(()),
        }
    }

    /// Proactive path: returns a valid token, refreshing from cache or falling
    /// back to the original source as needed.
    pub async fn get_valid_token(&self) -> Result<String, AppError> {
        let _guard = self.lock.lock().await;

        let now = now_ms();

        if let Some(cached) = self.cache.get_raw() {
            let ttl = cached.expires_at_ms - now;
            if ttl > PROACTIVE_REFRESH_MS {
                return Ok(cached.token);
            }
            if self.source.supports_refresh() && !cached.refresh_token.is_empty() {
                match self.source.refresh(&cached.refresh_token).await {
                    Ok(new_creds) => {
                        let _ = self.cache.put(&new_creds);
                        return Ok(new_creds.token);
                    }
                    Err(AppError::ReauthRequired(_)) => {}
                    Err(_) => {
                        if ttl > 0 {
                            return Ok(cached.token);
                        }
                    }
                }
            }
        }

        let fresh = self.source.fetch_from_source()?;
        if self.source.supports_refresh()
            && (fresh.expires_at_ms - now) <= PROACTIVE_REFRESH_MS
            && !fresh.refresh_token.is_empty()
        {
            if let Ok(new_creds) = self.source.refresh(&fresh.refresh_token).await {
                let _ = self.cache.put(&new_creds);
                return Ok(new_creds.token);
            }
        }
        if fresh.expires_at_ms > 0 {
            let _ = self.cache.put(&fresh);
        }
        Ok(fresh.token)
    }

    /// Reactive path: called after the usage API returns 401.
    /// Refreshes from cache first; only falls back to the source as a last resort.
    pub async fn recover_from_rejection(&self) -> Result<String, AppError> {
        let _guard = self.lock.lock().await;

        let now = now_ms();

        if let Some(cached) = self.cache.get_raw() {
            let ttl = cached.expires_at_ms - now;
            if ttl > PROACTIVE_REFRESH_MS {
                return Ok(cached.token);
            }
            if self.source.supports_refresh() && !cached.refresh_token.is_empty() {
                match self.source.refresh(&cached.refresh_token).await {
                    Ok(new_creds) => {
                        let _ = self.cache.put(&new_creds);
                        return Ok(new_creds.token);
                    }
                    Err(AppError::ReauthRequired(_)) => {}
                    Err(e) => return Err(e),
                }
            }
        }

        self.cache.clear()?;
        let fresh = self.source.fetch_from_source()?;
        if self.source.supports_refresh() && !fresh.refresh_token.is_empty() {
            if let Ok(new_creds) = self.source.refresh(&fresh.refresh_token).await {
                let _ = self.cache.put(&new_creds);
                return Ok(new_creds.token);
            }
        }
        if fresh.expires_at_ms > 0 {
            let _ = self.cache.put(&fresh);
        }
        Ok(fresh.token)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
