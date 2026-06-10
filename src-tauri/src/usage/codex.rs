use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use super::token_manager::{CredentialSource, TokenManager};
use super::{run_report_flow, CachedCredentials, UsageConnector, UsageReport, UsageWindow};
use crate::error::AppError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_TOKEN_TTL_S: i64 = 863_999;

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
    #[serde(default)]
    last_refresh: Option<String>,
}

#[derive(Deserialize)]
struct CodexTokens {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
}

#[derive(Deserialize)]
struct CodexRefreshResponse {
    access_token: String,
    expires_in: i64,
    refresh_token: String,
}

// ---------------------------------------------------------------------------
// JWT helpers
// ---------------------------------------------------------------------------

fn account_id_from_jwt(token: &str) -> Option<String> {
    let payload_b64 = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    json["https://api.openai.com/auth"]["chatgpt_account_id"]
        .as_str()
        .map(str::to_string)
}

// Credential source
// ---------------------------------------------------------------------------

struct CodexCredSource;

impl CodexCredSource {
    fn read_auth_file() -> Result<CachedCredentials, AppError> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Internal("unable to determine home directory".into()))?;

        let path = home.join(".codex/auth.json");
        let contents = std::fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::ReauthRequired("no credentials found at ~/.codex/auth.json".into())
            }
            _ => AppError::Internal(format!("could not read ~/.codex/auth.json: {e}")),
        })?;

        let auth: CodexAuthFile = serde_json::from_str(&contents).map_err(|_| {
            AppError::ReauthRequired(
                "unreadable credentials in ~/.codex/auth.json — re-authenticate in ChatGPT".into(),
            )
        })?;
        if auth.tokens.access_token.is_empty() {
            return Err(AppError::ReauthRequired(
                "incomplete credentials in ~/.codex/auth.json".into(),
            ));
        }

        let expires_at_ms = auth
            .last_refresh
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis() + CODEX_TOKEN_TTL_S * 1_000)
            .unwrap_or(0);

        Ok(CachedCredentials {
            token: auth.tokens.access_token,
            expires_at_ms,
            refresh_token: auth.tokens.refresh_token,
        })
    }
}

#[async_trait]
impl CredentialSource for CodexCredSource {
    fn provider(&self) -> &'static str {
        "codex"
    }

    fn supports_refresh(&self) -> bool {
        true
    }

    fn fetch_from_source(&self) -> Result<CachedCredentials, AppError> {
        Self::read_auth_file()
    }

    async fn refresh(&self, rt: &str) -> Result<CachedCredentials, AppError> {
        let client = crate::http_client::build_client()?;
        let resp = client
            .post("https://auth.openai.com/oauth/token")
            .header("accept", "*/*")
            .header("originator", "codex-tui")
            .header("user-agent", "codex-tui/0.137.0")
            .json(&serde_json::json!({
                "client_id": CODEX_CLIENT_ID,
                "grant_type": "refresh_token",
                "refresh_token": rt,
            }))
            .send()
            .await?;

        let status = resp.status();
        if status.is_client_error() {
            return Err(AppError::ReauthRequired(format!(
                "codex refresh rejected ({status}) — re-authenticate in ChatGPT"
            )));
        }
        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "codex refresh endpoint returned {status}"
            )));
        }

        let body: CodexRefreshResponse = resp.json().await.map_err(AppError::from)?;

        Ok(CachedCredentials {
            token: body.access_token,
            expires_at_ms: now_ms() + body.expires_in * 1_000,
            refresh_token: body.refresh_token,
        })
    }
}

// ---------------------------------------------------------------------------
// Connector
// ---------------------------------------------------------------------------

pub struct CodexConnector {
    tokens: TokenManager<CodexCredSource>,
    app: AppHandle,
}

impl CodexConnector {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        Ok(Self {
            tokens: TokenManager::new(CodexCredSource),
            app,
        })
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

        let client = crate::http_client::build_client()?;
        let resp = client
            .get("https://chatgpt.com/backend-api/wham/usage")
            .header("Authorization", format!("Bearer {token}"))
            .header("chatgpt-account-id", account_id)
            .send()
            .await?;
        eprintln!(
            "[timing] codex::http_request (send) = {:?}",
            _start.elapsed()
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
            "[timing] codex::http_request (total) = {:?}",
            _start.elapsed()
        );
        result
    }

    fn map_stats(stats: CodexUsageResponse) -> UsageReport {
        let five_hour_reset =
            chrono::DateTime::from_timestamp(stats.rate_limit.primary_window.reset_at, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".into());

        let seven_day_reset =
            chrono::DateTime::from_timestamp(stats.rate_limit.secondary_window.reset_at, 0)
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
        run_report_flow(
            "codex",
            &self.tokens,
            || self.read_usage_cache(),
            |r| self.write_usage_cache(r),
            |creds| async move {
                let account_id = account_id_from_jwt(&creds.token).unwrap_or_default();
                self.get_usage_stats(&creds.token, &account_id).await
            },
            Self::map_stats,
            "credentials expired — re-authenticate in ChatGPT",
            force_refresh,
        )
        .await
    }
}
