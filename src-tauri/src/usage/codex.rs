use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use super::{UsageConnector, UsageReport, UsageWindow};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Raw API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CodexUsageResponse {
    rate_limit: CodexRateLimit,
}

#[derive(Deserialize)]
struct CodexRateLimit {
    primary_window: CodexWindow,
    secondary_window: CodexWindow,
}

#[derive(Deserialize)]
struct CodexWindow {
    used_percent: f64,
    reset_at: i64,
}

// ---------------------------------------------------------------------------
// Credentials file shape
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CodexAuthFile {
    tokens: CodexTokens,
}

#[derive(Deserialize)]
struct CodexTokens {
    access_token: String,
    account_id: String,
}

// ---------------------------------------------------------------------------
// Connector
// ---------------------------------------------------------------------------

pub struct CodexConnector {
    app: AppHandle,
}

impl CodexConnector {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        Ok(Self { app })
    }

    // -----------------------------------------------------------------------
    // Credentials source (file only — no expiresAt to drive Stronghold)
    // -----------------------------------------------------------------------

    fn read_auth_file(&self) -> Result<(String, String), AppError> {
        let home = dirs::home_dir().ok_or_else(|| {
            AppError::Internal("unable to determine home directory".into())
        })?;

        let path = home.join(".codex/auth.json");
        let contents = std::fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::ReauthRequired("no credentials found at ~/.codex/auth.json".into())
            }
            _ => AppError::Internal(format!("could not read ~/.codex/auth.json: {e}")),
        })?;

        let auth: CodexAuthFile = serde_json::from_str(&contents)?;
        if auth.tokens.access_token.is_empty() || auth.tokens.account_id.is_empty() {
            return Err(AppError::ReauthRequired(
                "incomplete credentials in ~/.codex/auth.json".into(),
            ));
        }

        Ok((auth.tokens.access_token, auth.tokens.account_id))
    }

    // -----------------------------------------------------------------------
    // Usage cache (tauri-plugin-store, plain JSON)
    // -----------------------------------------------------------------------

    fn read_usage_cache(&self) -> Option<UsageReport> {
        let store = self.app.store("usage-cache.json").ok()?;
        let val = store.get("codex")?;
        serde_json::from_value(val).ok()
    }

    fn write_usage_cache(&self, report: &UsageReport) {
        if let Ok(store) = self.app.store("usage-cache.json") {
            if let Ok(val) = serde_json::to_value(report) {
                store.set("codex", val);
                let _ = store.save();
            }
        }
    }

    // -----------------------------------------------------------------------
    // Live API call
    // -----------------------------------------------------------------------

    async fn get_usage_stats(
        &self,
        token: &str,
        account_id: &str,
    ) -> Result<CodexUsageResponse, AppError> {
        let _start = Instant::now();

        let client = reqwest::Client::new();
        let resp = client
            .get("https://chatgpt.com/backend-api/wham/usage")
            .header("Authorization", format!("Bearer {token}"))
            .header("chatgpt-account-id", account_id)
            .send()
            .await?;
        eprintln!("[timing] codex::http_request (send) = {:?}", _start.elapsed());

        let status = resp.status();

        if status.is_client_error() {
            return Err(AppError::Unauthorized(format!("API rejected the token ({status})")));
        }

        if !status.is_success() {
            return Err(AppError::Internal(format!("API returned {status}")));
        }

        let result = resp.json().await.map_err(AppError::from);
        eprintln!("[timing] codex::http_request (total) = {:?}", _start.elapsed());
        result
    }

    fn map_stats(stats: CodexUsageResponse) -> UsageReport {
        let five_hour_reset = chrono::DateTime::from_timestamp(
            stats.rate_limit.primary_window.reset_at,
            0,
        )
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "unknown".into());

        let seven_day_reset = chrono::DateTime::from_timestamp(
            stats.rate_limit.secondary_window.reset_at,
            0,
        )
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "unknown".into());

        UsageReport {
            provider_name: "codex".into(),
            five_hour: UsageWindow {
                utilization: stats.rate_limit.primary_window.used_percent / 100.0,
                resets_at: five_hour_reset,
            },
            seven_day: UsageWindow {
                utilization: stats.rate_limit.secondary_window.used_percent / 100.0,
                resets_at: seven_day_reset,
            },
            fetched_at_ms: now_ms(),
            cached: false,
            metadata: None,
        }
    }
}

const CACHE_TTL_MS: i64 = 5 * 60 * 1_000;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[async_trait]
impl UsageConnector for CodexConnector {
    fn provider_name(&self) -> &'static str {
        "codex"
    }

    async fn generate_report(&self, force_refresh: bool) -> Result<UsageReport, AppError> {
        // Serve from cache if fresh and not a forced refresh
        if !force_refresh {
            if let Some(cached) = self.read_usage_cache() {
                if now_ms() - cached.fetched_at_ms < CACHE_TTL_MS {
                    return Ok(UsageReport {
                        cached: true,
                        ..cached
                    });
                }
            }
        }

        let start = Instant::now();

        // 1. Get credentials from file
        let _t1 = Instant::now();
        let (token, account_id) = self.read_auth_file()?;
        eprintln!(
            "[timing] codex::generate_report read_auth_file = {:?}",
            _t1.elapsed()
        );

        // 2. Try the live API
        let t2 = Instant::now();
        let result = self.get_usage_stats(&token, &account_id).await;
        eprintln!(
            "[timing] codex::generate_report get_usage_stats = {:?}",
            t2.elapsed()
        );

        match result {
            Ok(stats) => {
                let t3 = Instant::now();
                let report = Self::map_stats(stats);
                self.write_usage_cache(&report);
                eprintln!(
                    "[timing] codex::generate_report write_cache = {:?}",
                    t3.elapsed()
                );
                eprintln!(
                    "[timing] codex::generate_report (total, success) = {:?}",
                    start.elapsed()
                );
                Ok(report)
            }

            Err(AppError::Unauthorized(_)) => {
                // 3. Token rejected — re-read the file and retry once
                let t3 = Instant::now();
                match self.read_auth_file() {
                    Ok((new_token, new_account_id)) => {
                        eprintln!(
                            "[timing] codex::generate_report retry_fetch = {:?}",
                            t3.elapsed()
                        );
                        match self.get_usage_stats(&new_token, &new_account_id).await {
                            Ok(stats) => {
                                let report = Self::map_stats(stats);
                                self.write_usage_cache(&report);
                                eprintln!(
                                    "[timing] codex::generate_report (total, retry ok) = {:?}",
                                    start.elapsed()
                                );
                                Ok(report)
                            }
                            Err(AppError::Unauthorized(_)) => {
                                eprintln!(
                                    "[timing] codex::generate_report (total, reauth) = {:?}",
                                    start.elapsed()
                                );
                                Err(AppError::ReauthRequired(
                                    "credentials expired — re-authenticate in ChatGPT".into(),
                                ))
                            }
                            Err(AppError::Internal(msg)) => Err(AppError::Internal(msg)),
                            Err(e) => Err(e),
                        }
                    }
                    Err(AppError::ReauthRequired(msg)) => Err(AppError::ReauthRequired(msg)),
                    Err(e) => Err(e),
                }
            }

            Err(AppError::Internal(msg)) => {
                // 4. Network / 5xx error — fall back to cached usage
                if let Some(cached) = self.read_usage_cache() {
                    eprintln!(
                        "[timing] codex::generate_report (total, cached fallback) = {:?}",
                        start.elapsed()
                    );
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
