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
    /// MUST return AppError::ReauthRequired on a non-5xx client error (4xx)
    /// from the token endpoint (refresh token is dead). Network/5xx => AppError::Internal.
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

    /// Proactive path: returns valid credentials, refreshing from cache or
    /// falling back to the original source as needed.
    pub async fn get_valid_token(&self) -> Result<CachedCredentials, AppError> {
        let _guard = self.lock.lock().await;

        let now = now_ms();

        if let Some(cached) = self.cache.get_raw() {
            // expires_at_ms == 0 means "unknown expiry" (e.g. codex) —
            // serve as-is without proactive refresh; the reactive path will
            // re-read the source on 401.
            if cached.expires_at_ms == 0 {
                return Ok(cached);
            }

            let ttl = cached.expires_at_ms - now;
            if ttl > PROACTIVE_REFRESH_MS {
                return Ok(cached);
            }
            if self.source.supports_refresh() && !cached.refresh_token.is_empty() {
                match self.source.refresh(&cached.refresh_token).await {
                    Ok(new_creds) => {
                        if let Err(e) = self.cache.put(&new_creds) {
                            eprintln!("[token_manager] cache write failed: {e}");
                        }
                        return Ok(new_creds);
                    }
                    Err(AppError::ReauthRequired(_)) => {
                        if let Err(e) = self.cache.clear() {
                            eprintln!("[token_manager] cache clear failed: {e}");
                        }
                    }
                    Err(e) => {
                        if ttl > 0 {
                            return Ok(cached);
                        }
                        return Err(e);
                    }
                }
            }
        }

        let fresh = self.source.fetch_from_source()?;
        self.cache_and_return_source(fresh, now).await
    }

    /// Reactive path: called after the usage API returns 401.
    /// Refreshes from cache first; only falls back to the source as a last resort.
    pub async fn recover_from_rejection(
        &self,
        rejected_token: &str,
    ) -> Result<CachedCredentials, AppError> {
        let _guard = self.lock.lock().await;

        let now = now_ms();

        if let Some(cached) = self.cache.get_raw() {
            if cached.expires_at_ms == 0 {
                // Unknown expiry — cannot trust the rejected token is still
                // the same one in cache.  Re-read source.
            } else {
                let ttl = cached.expires_at_ms - now;
                if ttl > PROACTIVE_REFRESH_MS && cached.token != rejected_token {
                    return Ok(cached);
                }
            }
            if self.source.supports_refresh() && !cached.refresh_token.is_empty() {
                match self.source.refresh(&cached.refresh_token).await {
                    Ok(new_creds) => {
                        if let Err(e) = self.cache.put(&new_creds) {
                            eprintln!("[token_manager] cache write failed: {e}");
                        }
                        return Ok(new_creds);
                    }
                    Err(AppError::ReauthRequired(_)) => {}
                    Err(e) => return Err(e),
                }
            }
        }

        if let Err(e) = self.cache.clear() {
            eprintln!("[token_manager] cache clear failed: {e}");
        }
        let fresh = self.source.fetch_from_source()?;
        self.cache_and_return_source(fresh, now).await
    }

    /// Shared helper for the source-fallback path (used by both
    /// `get_valid_token` and `recover_from_rejection`).
    ///
    /// * If the source AT is expired and refresh fails → ReauthRequired.
    /// * If the source AT is alive (or has unknown expiry) → cache + return.
    async fn cache_and_return_source(
        &self,
        fresh: CachedCredentials,
        now: i64,
    ) -> Result<CachedCredentials, AppError> {
        let alive = fresh.expires_at_ms == 0 || fresh.expires_at_ms > now;

        if !alive && self.source.supports_refresh() && !fresh.refresh_token.is_empty() {
            let rt = fresh.refresh_token.clone();
            match self.source.refresh(&rt).await {
                Ok(new_creds) => {
                    if let Err(e) = self.cache.put(&new_creds) {
                        eprintln!("[token_manager] cache write failed: {e}");
                    }
                    return Ok(new_creds);
                }
                Err(AppError::ReauthRequired(_)) => {
                    return Err(AppError::ReauthRequired(
                        "credentials expired — reauthenticate".into(),
                    ));
                }
                Err(e) => return Err(e),
            }
        }

        if alive {
            if let Err(e) = self.cache.put(&fresh) {
                eprintln!("[token_manager] cache write failed: {e}");
            }
            Ok(fresh)
        } else {
            Err(AppError::ReauthRequired(
                "credentials expired — reauthenticate".into(),
            ))
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
