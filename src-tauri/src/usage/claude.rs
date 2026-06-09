use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use super::{CachedCredentials, CredsCache};
use super::{UsageConnector, UsageReport, UsageWindow};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Raw API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ClaudeUsageResponse {
    five_hour: ClaudeWindow,
    seven_day: ClaudeWindow,
}

#[derive(Deserialize)]
struct ClaudeWindow {
    utilization: f64,
    resets_at: String,
}

// ---------------------------------------------------------------------------
// Credentials file shape
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeAiOauth>,
}

#[derive(Deserialize)]
struct ClaudeAiOauth {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

// ---------------------------------------------------------------------------
// Connector
// ---------------------------------------------------------------------------

pub struct ClaudeConnector {
    creds: CredsCache,
    app: AppHandle,
}

impl ClaudeConnector {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        Ok(Self {
            creds: CredsCache::new("claude"),
            app,
        })
    }

    // -----------------------------------------------------------------------
    // Credentials source (keychain → file)
    // -----------------------------------------------------------------------

    fn try_keychain(service: &str, account: &str) -> Option<String> {
        keyring::Entry::new(service, account)
            .ok()?
            .get_password()
            .ok()
    }

    fn parse_auth_file(&self, contents: &str) -> Result<CachedCredentials, AppError> {
        let creds: CredentialsFile = serde_json::from_str(contents)?;
        let oauth = creds
            .claude_ai_oauth
            .ok_or_else(|| AppError::Unauthorized("missing claudeAiOauth in credentials".into()))?;
        let token = oauth.access_token.ok_or_else(|| {
            AppError::Unauthorized("missing accessToken in credentials".into())
        })?;
        if token.is_empty() {
            return Err(AppError::Unauthorized("empty accessToken in credentials".into()));
        }
        let expires_at_ms = oauth.expires_at.unwrap_or(0);
        Ok(CachedCredentials {
            token,
            expires_at_ms,
        })
    }

    fn read_auth_file(&self) -> Result<String, AppError> {
        let home = dirs::home_dir().ok_or_else(|| {
            AppError::Internal("unable to determine home directory".into())
        })?;

        let paths = [
            home.join(".claude/.credentials.json"),
            home.join(".claude/credentials.json"),
        ];

        for path in &paths {
            if path.exists() {
                return std::fs::read_to_string(path).map_err(AppError::from);
            }
        }

        Err(AppError::Unauthorized(
            "no credentials file found at ~/.claude/.credentials.json or ~/.claude/credentials.json"
                .into(),
        ))
    }

    fn fetch_credentials_from_source(&self) -> Result<CachedCredentials, AppError> {
        let user = std::env::var("USER").unwrap_or_else(|_| "claude".to_string());

        let blob = Self::try_keychain("Claude Code-credentials", &user)
            .or_else(|| Self::try_keychain("Claude Code", &user))
            .or_else(|| self.read_auth_file().ok());

        match blob {
            Some(contents) => self.parse_auth_file(&contents),
            None => Err(AppError::ReauthRequired(
                "no credentials found in keychain or file — run `claude login` in your terminal"
                    .into(),
            )),
        }
    }

    // -----------------------------------------------------------------------
    // Token lifecycle with Stronghold-backed cache
    // -----------------------------------------------------------------------

    fn get_valid_token(&self) -> Result<String, AppError> {
        let start = Instant::now();

        // Try cache first
        let t0 = Instant::now();
        if let Some(cached) = self.creds.get() {
            eprintln!("[timing] claude::get_valid_token cache_hit = {:?}", t0.elapsed());
            eprintln!("[timing] claude::get_valid_token (total cached) = {:?}", start.elapsed());
            return Ok(cached.token);
        }
        eprintln!("[timing] claude::get_valid_token cache_miss = {:?}", t0.elapsed());

        let t1 = Instant::now();
        let fresh = self.fetch_credentials_from_source()?;
        eprintln!("[timing] claude::get_valid_token fetch_source = {:?}", t1.elapsed());

        // Only persist to Stronghold when we have a real expiry
        if fresh.expires_at_ms > 0 {
            let t2 = Instant::now();
            self.creds.put(&fresh)?;
            eprintln!("[timing] claude::get_valid_token creds_put = {:?}", t2.elapsed());
        }

        eprintln!("[timing] claude::get_valid_token (total fetch) = {:?}", start.elapsed());
        Ok(fresh.token)
    }

    // -----------------------------------------------------------------------
    // Usage cache (tauri-plugin-store, plain JSON)
    // -----------------------------------------------------------------------

    fn read_usage_cache(&self) -> Option<UsageReport> {
        let store = self.app.store("usage-cache.json").ok()?;
        let val = store.get("claude")?;
        serde_json::from_value(val).ok()
    }

    fn write_usage_cache(&self, report: &UsageReport) {
        if let Ok(store) = self.app.store("usage-cache.json") {
            if let Ok(val) = serde_json::to_value(report) {
                store.set("claude", val);
                let _ = store.save();
            }
        }
    }

    // -----------------------------------------------------------------------
    // Live API call
    // -----------------------------------------------------------------------

    async fn get_usage_stats(&self, token: &str) -> Result<ClaudeUsageResponse, AppError> {
        let start = Instant::now();

        let client = reqwest::Client::new();
        let resp = client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("Authorization", format!("Bearer {token}"))
            .header("anthropic-beta", "oauth-2025-04-20")
            .send()
            .await?;
        eprintln!("[timing] claude::http_request (send) = {:?}", start.elapsed());

        let status = resp.status();

        if status.is_client_error() {
            return Err(AppError::Unauthorized(format!("API rejected the token ({status})")));
        }

        if !status.is_success() {
            return Err(AppError::Internal(format!("API returned {status}")));
        }

        let result = resp.json().await.map_err(AppError::from);
        eprintln!("[timing] claude::http_request (total) = {:?}", start.elapsed());
        result
    }

    fn map_stats(stats: ClaudeUsageResponse) -> UsageReport {
        UsageReport {
            provider_name: "claude".into(),
            five_hour: UsageWindow {
                utilization: stats.five_hour.utilization,
                resets_at: stats.five_hour.resets_at,
            },
            seven_day: UsageWindow {
                utilization: stats.seven_day.utilization,
                resets_at: stats.seven_day.resets_at,
            },
            fetched_at_ms: now_ms(),
            cached: false,
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[async_trait]
impl UsageConnector for ClaudeConnector {
    fn provider_name(&self) -> &'static str {
        "claude"
    }

    async fn generate_report(&self) -> Result<UsageReport, AppError> {
        let start = Instant::now();

        // 1. Get a valid token (cached → source)
        let _t1 = Instant::now();
        let token = self.get_valid_token()?;
        eprintln!("[timing] claude::generate_report get_valid_token = {:?}", _t1.elapsed());

        // 2. Try the live API
        let t2 = Instant::now();
        let result = self.get_usage_stats(&token).await;
        eprintln!("[timing] claude::generate_report get_usage_stats = {:?}", t2.elapsed());

        match result {
            Ok(stats) => {
                let t3 = Instant::now();
                let report = Self::map_stats(stats);
                self.write_usage_cache(&report);
                eprintln!("[timing] claude::generate_report write_cache = {:?}", t3.elapsed());
                eprintln!("[timing] claude::generate_report (total, success) = {:?}", start.elapsed());
                Ok(report)
            }

            Err(AppError::Unauthorized(_)) => {
                // 3. Token rejected — invalidate Stronghold cache and retry once
                let t3 = Instant::now();
                let _ = self.creds.clear();
                eprintln!("[timing] claude::generate_report creds_clear = {:?}", t3.elapsed());

                let t4 = Instant::now();
                let token = match self.fetch_credentials_from_source() {
                    Ok(fresh) => {
                        if fresh.expires_at_ms > 0 {
                            let _ = self.creds.put(&fresh);
                        }
                        fresh.token
                    }
                    Err(AppError::ReauthRequired(msg)) => {
                        return Err(AppError::ReauthRequired(msg));
                    }
                    Err(e) => return Err(e),
                };
                eprintln!("[timing] claude::generate_report retry_fetch = {:?}", t4.elapsed());

                // Retry once
                match self.get_usage_stats(&token).await {
                    Ok(stats) => {
                        let report = Self::map_stats(stats);
                        self.write_usage_cache(&report);
                        eprintln!("[timing] claude::generate_report (total, retry ok) = {:?}", start.elapsed());
                        Ok(report)
                    }
                    Err(AppError::Unauthorized(_)) => {
                        eprintln!("[timing] claude::generate_report (total, reauth) = {:?}", start.elapsed());
                        Err(AppError::ReauthRequired(
                            "credentials expired — run `claude login` in your terminal".into(),
                        ))
                    }
                    Err(AppError::Internal(msg)) => Err(AppError::Internal(msg)),
                    Err(e) => Err(e),
                }
            }

            Err(AppError::Internal(msg)) => {
                // 4. Network / 5xx error — fall back to cached usage
                if let Some(cached) = self.read_usage_cache() {
                    eprintln!("[timing] claude::generate_report (total, cached fallback) = {:?}", start.elapsed());
                    Ok(UsageReport {
                        cached: true,
                        ..cached
                    })
                } else {
                    Err(AppError::Internal(format!(
                        "API unavailable and no cached report available: {msg}"
                    )))
                }
            }

            Err(e) => Err(e),
        }
    }
}
