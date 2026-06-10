use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use super::token_manager::{CredentialSource, TokenManager};
use super::{run_report_flow, CachedCredentials, UsageConnector, UsageReport, UsageWindow};
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
    #[serde(rename = "refreshToken")]
    refresh_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Refresh response type
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ClaudeRefreshResponse {
    access_token: String,
    expires_in: i64,
    #[serde(default)]
    refresh_token: Option<String>,
}

const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_SCOPE: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

// ---------------------------------------------------------------------------
// Credential source
// ---------------------------------------------------------------------------

struct ClaudeCredSource;

impl ClaudeCredSource {
    fn try_keychain(service: &str, account: &str) -> Result<Option<String>, AppError> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| AppError::Internal(format!("claude keychain entry failed: {e}")))?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Internal(format!(
                "claude keychain access failed: {e}"
            ))),
        }
    }

    fn parse_auth_file(contents: &str) -> Result<CachedCredentials, AppError> {
        let creds: CredentialsFile = serde_json::from_str(contents).map_err(AppError::from)?;
        let oauth = creds.claude_ai_oauth.ok_or_else(|| {
            AppError::ReauthRequired("missing claudeAiOauth in credentials".into())
        })?;
        let token = oauth
            .access_token
            .ok_or_else(|| AppError::ReauthRequired("missing accessToken in credentials".into()))?;
        if token.is_empty() {
            return Err(AppError::ReauthRequired(
                "empty accessToken in credentials".into(),
            ));
        }
        let expires_at_ms = oauth.expires_at.unwrap_or(0);
        Ok(CachedCredentials {
            token,
            expires_at_ms,
            refresh_token: oauth.refresh_token.unwrap_or_default(),
        })
    }

    fn read_auth_file() -> Result<String, AppError> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Internal("unable to determine home directory".into()))?;

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

    fn fetch_credentials_from_source() -> Result<CachedCredentials, AppError> {
        let user = std::env::var("USER").unwrap_or_else(|_| "claude".to_string());

        let mut source_errors: Vec<String> = Vec::new();

        for (service, account) in [("Claude Code-credentials", &user), ("Claude Code", &user)] {
            match Self::try_keychain(service, account) {
                Ok(Some(contents)) => return Self::parse_auth_file(&contents),
                Ok(None) => continue,
                Err(e) => {
                    source_errors.push(format!("keychain({service}/{account}): {e}"));
                    continue;
                }
            }
        }

        match Self::read_auth_file() {
            Ok(contents) => return Self::parse_auth_file(&contents),
            Err(AppError::Unauthorized(_)) => {}
            Err(e) => {
                source_errors.push(format!("file: {e}"));
            }
        }

        if source_errors.is_empty() {
            Err(AppError::ReauthRequired(
                "no credentials found in keychain or file — run `claude login` in your terminal"
                    .into(),
            ))
        } else {
            Err(AppError::Internal(format!(
                "credentials unavailable: {}",
                source_errors.join("; ")
            )))
        }
    }
}

#[async_trait]
impl CredentialSource for ClaudeCredSource {
    fn provider(&self) -> &'static str {
        "claude"
    }

    fn supports_refresh(&self) -> bool {
        true
    }

    fn fetch_from_source(&self) -> Result<CachedCredentials, AppError> {
        Self::fetch_credentials_from_source()
    }

    async fn refresh(&self, rt: &str) -> Result<CachedCredentials, AppError> {
        let client = crate::http_client::build_client()?;
        let resp = client
            .post("https://platform.claude.com/v1/oauth/token")
            .header("Accept", "application/json, text/plain, */*")
            .header("User-Agent", "axios/1.15.2")
            .json(&serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": rt,
                "client_id": CLAUDE_CLIENT_ID,
                "scope": CLAUDE_SCOPE,
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("token refresh request failed: {e}")))?;

        let status = resp.status();
        if status.is_client_error() {
            return Err(AppError::ReauthRequired(
                "refresh token rejected — run `claude login` in your terminal".into(),
            ));
        }
        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "token refresh failed: {status}"
            )));
        }

        let body: ClaudeRefreshResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("token refresh parse failed: {e}")))?;
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

pub struct ClaudeConnector {
    tokens: TokenManager<ClaudeCredSource>,
    app: AppHandle,
}

impl ClaudeConnector {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        Ok(Self {
            tokens: TokenManager::new(ClaudeCredSource),
            app,
        })
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

        let client = crate::http_client::build_client()?;
        let resp = client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("Authorization", format!("Bearer {token}"))
            .header("anthropic-beta", "oauth-2025-04-20")
            .send()
            .await?;
        eprintln!(
            "[timing] claude::http_request (send) = {:?}",
            start.elapsed()
        );

        let status = resp.status();

        match status.as_u16() {
            401 | 403 => {
                return Err(AppError::Unauthorized(format!(
                    "API rejected the token ({status})"
                )));
            }
            _ if status.is_client_error() => {
                return Err(AppError::Internal(format!(
                    "usage API returned client error {status}"
                )));
            }
            _ if !status.is_success() => {
                return Err(AppError::Internal(format!("API returned {status}")));
            }
            _ => {}
        }

        let result = resp.json().await.map_err(AppError::from);
        eprintln!(
            "[timing] claude::http_request (total) = {:?}",
            start.elapsed()
        );
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
            metadata: None,
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

    async fn generate_report(&self, force_refresh: bool) -> Result<UsageReport, AppError> {
        run_report_flow(
            "claude",
            &self.tokens,
            || self.read_usage_cache(),
            |r| self.write_usage_cache(r),
            |creds| async move { self.get_usage_stats(&creds.token).await },
            Self::map_stats,
            "credentials expired — run `claude login` in your terminal",
            force_refresh,
        )
        .await
    }
}
