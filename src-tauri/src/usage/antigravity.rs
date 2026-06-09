use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use super::token_manager::{CredentialSource, TokenManager};
use super::{CachedCredentials, UsageConnector, UsageMetadata, UsageReport, UsageWindow};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Raw API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct LoadCodeAssistResponse {
    #[serde(rename = "cloudaicompanionProject")]
    cloudaicompanion_project: String,
}

#[derive(Deserialize)]
struct QuotaSummaryResponse {
    buckets: Vec<Bucket>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Bucket {
    #[serde(rename = "bucketId")]
    bucket_id: String,
    #[serde(rename = "displayName")]
    display_name: String,
    window: String,
    #[serde(rename = "resetTime")]
    reset_time: String,
    #[serde(rename = "remainingFraction")]
    remaining_fraction: f64,
}

// ---------------------------------------------------------------------------
// Credentials keychain shape (nested under "token")
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AntigravityAuth {
    token: AntigravityToken,
}

#[derive(Deserialize)]
struct AntigravityToken {
    access_token: String,
    #[serde(default)]
    expiry: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct RefreshTokenResponse {
    access_token: String,
    expires_in: i64,
    #[serde(default)]
    refresh_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Credential source
// ---------------------------------------------------------------------------

struct AntigravityCredSource;

impl AntigravityCredSource {
    const KEYCHAIN_SERVICE: &'static str = "gemini";
    const KEYCHAIN_ACCOUNT: &'static str = "antigravity";
    const GOKEYRING_PREFIX: &'static str = "go-keyring-base64:";
    const OAUTH_TOKEN_URL: &'static str = "https://oauth2.googleapis.com/token";

    fn oauth_client_id() -> Result<String, AppError> {
        #[cfg(not(debug_assertions))]
        {
            Ok(env!("ANTIGRAVITY_CLIENT_ID").to_string())
        }
        #[cfg(debug_assertions)]
        {
            std::env::var("ANTIGRAVITY_CLIENT_ID")
                .map_err(|_| AppError::Internal("ANTIGRAVITY_CLIENT_ID not set in .env".into()))
        }
    }

    fn oauth_client_secret() -> Result<String, AppError> {
        #[cfg(not(debug_assertions))]
        {
            Ok(env!("ANTIGRAVITY_CLIENT_SECRET").to_string())
        }
        #[cfg(debug_assertions)]
        {
            std::env::var("ANTIGRAVITY_CLIENT_SECRET")
                .map_err(|_| AppError::Internal("ANTIGRAVITY_CLIENT_SECRET not set in .env".into()))
        }
    }

    fn parse_credentials(raw: &str) -> Result<CachedCredentials, AppError> {
        let json = match raw.strip_prefix(Self::GOKEYRING_PREFIX) {
            Some(b64) => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64.trim())
                    .map_err(|e| AppError::Internal(format!("base64 decode failed: {e}")))?;
                String::from_utf8(bytes)
                    .map_err(|e| AppError::Internal(format!("utf8 decode failed: {e}")))?
            }
            None => raw.to_string(),
        };

        let auth: AntigravityAuth = serde_json::from_str(&json)?;
        if auth.token.access_token.is_empty() {
            return Err(AppError::ReauthRequired(
                "empty access_token in antigravity keychain entry".into(),
            ));
        }
        let expires_at_ms = auth
            .token
            .expiry
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(0);
        Ok(CachedCredentials {
            token: auth.token.access_token,
            expires_at_ms,
            refresh_token: auth.token.refresh_token.unwrap_or_default(),
        })
    }
}

#[async_trait]
impl CredentialSource for AntigravityCredSource {
    fn provider(&self) -> &'static str {
        "antigravity"
    }

    fn supports_refresh(&self) -> bool {
        true
    }

    fn fetch_from_source(&self) -> Result<CachedCredentials, AppError> {
        let entry = keyring::Entry::new(Self::KEYCHAIN_SERVICE, Self::KEYCHAIN_ACCOUNT)
            .map_err(|e| AppError::Internal(format!("antigravity keychain entry failed: {e}")))?;

        let raw = match entry.get_password() {
            Ok(pw) => pw,
            Err(keyring::Error::NoEntry) => {
                return Err(AppError::ReauthRequired(
                    "no antigravity credentials in keychain — re-authenticate with the Antigravity CLI"
                        .into(),
                ));
            }
            Err(e) => {
                return Err(AppError::Internal(format!(
                    "antigravity keychain access failed: {e}"
                )));
            }
        };
        Self::parse_credentials(&raw)
    }

    async fn refresh(&self, rt: &str) -> Result<CachedCredentials, AppError> {
        let client = reqwest::Client::new();
        let client_id = Self::oauth_client_id()?;
        let client_secret = Self::oauth_client_secret()?;
        let resp = client
            .post(Self::OAUTH_TOKEN_URL)
            .header("User-Agent", "Go-http-client/2.0")
            .form(&[
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", rt),
            ])
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 400 || status.as_u16() == 401 {
            return Err(AppError::ReauthRequired(
                "refresh token rejected — re-authenticate with the Antigravity CLI".into(),
            ));
        }
        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "token refresh failed: {status}"
            )));
        }

        let body: RefreshTokenResponse = resp.json().await?;
        Ok(CachedCredentials {
            token: body.access_token,
            expires_at_ms: now_ms() + body.expires_in * 1_000,
            refresh_token: body.refresh_token.unwrap_or_else(|| rt.to_string()),
        })
    }
}

// ---------------------------------------------------------------------------
// Connector
// ---------------------------------------------------------------------------

pub struct AntigravityConnector {
    tokens: TokenManager<AntigravityCredSource>,
    app: AppHandle,
}

impl AntigravityConnector {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        Ok(Self {
            tokens: TokenManager::new(AntigravityCredSource),
            app,
        })
    }

    // -----------------------------------------------------------------------
    // Usage cache (tauri-plugin-store, plain JSON)
    // -----------------------------------------------------------------------

    fn read_usage_cache(&self) -> Option<UsageReport> {
        let store = self.app.store("usage-cache.json").ok()?;
        let val = store.get("antigravity")?;
        serde_json::from_value(val).ok()
    }

    fn write_usage_cache(&self, report: &UsageReport) {
        if let Ok(store) = self.app.store("usage-cache.json") {
            if let Ok(val) = serde_json::to_value(report) {
                store.set("antigravity", val);
                let _ = store.save();
            }
        }
    }

    // -----------------------------------------------------------------------
    // Live API calls (two sequential POSTs)
    // -----------------------------------------------------------------------

    async fn get_usage_stats(&self, token: &str) -> Result<QuotaSummaryResponse, AppError> {
        let client = reqwest::Client::new();

        // 1. LoadCodeAssist -> get project name
        let resp = client
            .post("https://daily-cloudcode-pa.googleapis.com/v1internal:loadCodeAssist")
            .header("User-Agent", "antigravity/cli/1.0.6 darwin/arm64")
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(r#"{"metadata":{"ideType":"ANTIGRAVITY"}}"#)
            .send()
            .await?;

        let status = resp.status();

        if status.is_client_error() {
            return Err(AppError::Unauthorized(format!(
                "API rejected the token ({status})"
            )));
        }

        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "loadCodeAssist returned {status}"
            )));
        }

        let load_resp: LoadCodeAssistResponse = resp.json().await.map_err(AppError::from)?;

        // 2. RetrieveUserQuotaSummary -> get buckets
        let body = serde_json::json!({ "project": load_resp.cloudaicompanion_project });
        let resp = client
            .post("https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary")
            .header("User-Agent", "antigravity/cli/1.0.6 darwin/arm64")
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();

        if status.is_client_error() {
            return Err(AppError::Unauthorized(format!(
                "API rejected the token ({status})"
            )));
        }

        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "retrieveUserQuotaSummary returned {status}"
            )));
        }

        resp.json().await.map_err(AppError::from)
    }

    fn map_stats(buckets: Vec<Bucket>) -> UsageReport {
        let mut primary_label = String::from("Gemini Models");
        let mut secondary_label = String::from("Claude/GPT Models");

        let mut gemini_5h = UsageWindow {
            utilization: 0.0,
            resets_at: "unknown".into(),
        };
        let mut gemini_weekly = UsageWindow {
            utilization: 0.0,
            resets_at: "unknown".into(),
        };
        let mut third_5h = UsageWindow {
            utilization: 0.0,
            resets_at: "unknown".into(),
        };
        let mut third_weekly = UsageWindow {
            utilization: 0.0,
            resets_at: "unknown".into(),
        };

        for b in &buckets {
            let window = UsageWindow {
                utilization: (1.0 - b.remaining_fraction) * 100.0,
                resets_at: b.reset_time.clone(),
            };
            match b.bucket_id.as_str() {
                "gemini-5h" => {
                    primary_label = b.display_name.clone();
                    gemini_5h = window;
                }
                "gemini-weekly" => {
                    primary_label = b.display_name.clone();
                    gemini_weekly = window;
                }
                "3p-5h" => {
                    secondary_label = b.display_name.clone();
                    third_5h = window;
                }
                "3p-weekly" => {
                    secondary_label = b.display_name.clone();
                    third_weekly = window;
                }
                _ => {}
            }
        }

        UsageReport {
            provider_name: "antigravity".into(),
            five_hour: gemini_5h,
            seven_day: gemini_weekly,
            fetched_at_ms: now_ms(),
            cached: false,
            metadata: Some(UsageMetadata {
                primary_label,
                secondary_label,
                secondary_five_hour: third_5h,
                secondary_seven_day: third_weekly,
            }),
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
impl UsageConnector for AntigravityConnector {
    fn provider_name(&self) -> &'static str {
        "antigravity"
    }

    async fn generate_report(&self, force_refresh: bool) -> Result<UsageReport, AppError> {
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

        let _t1 = Instant::now();
        let token = self.tokens.get_valid_token().await?;
        eprintln!(
            "[timing] antigravity::generate_report get_valid_token = {:?}",
            _t1.elapsed()
        );

        let t2 = Instant::now();
        let result = self.get_usage_stats(&token).await;
        eprintln!(
            "[timing] antigravity::generate_report get_usage_stats = {:?}",
            t2.elapsed()
        );

        match result {
            Ok(stats) => {
                let t3 = Instant::now();
                let report = Self::map_stats(stats.buckets);
                self.write_usage_cache(&report);
                eprintln!(
                    "[timing] antigravity::generate_report write_cache = {:?}",
                    t3.elapsed()
                );
                eprintln!(
                    "[timing] antigravity::generate_report (total, success) = {:?}",
                    start.elapsed()
                );
                Ok(report)
            }

            Err(AppError::Unauthorized(_)) => {
                let token = match self.tokens.recover_from_rejection().await {
                    Ok(t) => t,
                    Err(AppError::ReauthRequired(msg)) => {
                        return Err(AppError::ReauthRequired(msg));
                    }
                    Err(e) => return Err(e),
                };

                match self.get_usage_stats(&token).await {
                    Ok(stats) => {
                        let report = Self::map_stats(stats.buckets);
                        self.write_usage_cache(&report);
                        eprintln!(
                            "[timing] antigravity::generate_report (total, retry ok) = {:?}",
                            start.elapsed()
                        );
                        Ok(report)
                    }
                    Err(AppError::Unauthorized(_)) => {
                        eprintln!(
                            "[timing] antigravity::generate_report (total, reauth) = {:?}",
                            start.elapsed()
                        );
                        Err(AppError::ReauthRequired(
                            "credentials expired — re-authenticate with the Antigravity CLI".into(),
                        ))
                    }
                    Err(AppError::Internal(msg)) => Err(AppError::Internal(msg)),
                    Err(e) => Err(e),
                }
            }

            Err(AppError::Internal(msg)) => {
                if let Some(cached) = self.read_usage_cache() {
                    eprintln!(
                        "[timing] antigravity::generate_report (total, cached fallback) = {:?}",
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

