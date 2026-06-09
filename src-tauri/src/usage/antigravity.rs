use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use super::{
    CachedCredentials, CredsCache, UsageConnector, UsageMetadata, UsageReport, UsageWindow,
};
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
    expiry: Option<String>, // RFC3339
}

// ---------------------------------------------------------------------------
// Connector
// ---------------------------------------------------------------------------

pub struct AntigravityConnector {
    creds: CredsCache,
    app: AppHandle,
}

impl AntigravityConnector {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        Ok(Self {
            creds: CredsCache::new("antigravity"),
            app,
        })
    }

    // -----------------------------------------------------------------------
    // Credentials source (macOS Keychain — no OAuth refresh)
    // -----------------------------------------------------------------------

    const KEYCHAIN_SERVICE: &'static str = "gemini";
    const KEYCHAIN_ACCOUNT: &'static str = "antigravity";
    const GOKEYRING_PREFIX: &'static str = "go-keyring-base64:";

    /// Read + decode the source `gemini`/`antigravity` keychain entry.
    fn fetch_credentials_from_source(&self) -> Result<CachedCredentials, AppError> {
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

    /// Pure: go-keyring-base64 prefix strip + decode + nested-JSON parse + expiry → ms.
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
        })
    }

    /// Cache hit → return; miss → read source → persist (only if expiry known).
    fn get_valid_token(&self) -> Result<String, AppError> {
        if let Some(cached) = self.creds.get() {
            return Ok(cached.token);
        }
        let fresh = self.fetch_credentials_from_source()?;
        if fresh.expires_at_ms > 0 {
            let _ = self.creds.put(&fresh);
        }
        Ok(fresh.token)
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

        // 1. LoadCodeAssist → get project name
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

        // 2. RetrieveUserQuotaSummary → get buckets
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
        let token = self.get_valid_token()?;
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
                let t3 = Instant::now();
                let _ = self.creds.clear();
                eprintln!(
                    "[timing] antigravity::generate_report creds_clear = {:?}",
                    t3.elapsed()
                );

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
                eprintln!(
                    "[timing] antigravity::generate_report retry_fetch = {:?}",
                    t4.elapsed()
                );

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_go_keyring_base64_blob() {
        let raw = "go-keyring-base64:eyJ0b2tlbiI6eyJhY2Nlc3NfdG9rZW4iOiJ5YTI5LmEwQVQzb05aX0V2clRKWlBwN3B0R2JDdHVBRGtISDZsY21veVUtTENXM2pSbzQ2LXZZRk83Wk43UEd0em1Zc3lWdXhlYnp5N2hIZWZOcXNkX3llSE9YOUVOLVdRYUZuRmxKZF8xcVc4YjZ5c1ZKWTZmc3k5Y0hKa2pXZndNNXpTT3hNUl9Sc0g5YXJJU3FpZTFDY0hDODBQWmpIWUFLeTdRckdqU2kwZGR1N2w3SmJLNXN1UHFwdUt1Q0dveHEtZl9zV2ppUDg3cTg3REx1YUNnWUtBZDhTQVJZU0ZRSEdYMk1pSHBiTTFsMWpxWjY5bEhMVnhuSE9RZzAyMTEiLCJ0b2tlbl90eXBlIjoiQmVhcmVyIiwicmVmcmVzaF90b2tlbiI6IjEvLzBnM3dQeVpFZnBXMnlDZ1lJQVJBQUdCQVNOd0YtTDlJckdPM3plNDZsd2N6TVQtOENuMlRqZmJhVW1UT3E4N20zcGJwNXpwaW9zaEFQTUdvTll1SWFtNWV3VEpETnc5QTFKQ2siLCJleHBpcnkiOiIyMDI2LTA2LTA5VDE4OjQxOjA3LjU2OTMxNCswNTozMCJ9LCJhdXRoX21ldGhvZCI6ImNvbnN1bWVyIn0=";
        let creds = AntigravityConnector::parse_credentials(raw).unwrap();
        assert!(creds.token.starts_with("ya29."));
        assert!(creds.expires_at_ms > 0);
    }
}
