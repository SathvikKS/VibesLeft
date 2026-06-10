use std::collections::HashMap;
use std::future::Future;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

mod antigravity;
mod claude;
mod codex;
mod creds_cache;
mod token_manager;

pub(super) use creds_cache::{CachedCredentials, CredsCache};
use token_manager::{CredentialSource, TokenManager};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageWindow {
    pub utilization: f64,
    pub resets_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetadata {
    pub primary_label: String,
    pub secondary_label: String,
    pub secondary_five_hour: UsageWindow,
    pub secondary_seven_day: UsageWindow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageReport {
    pub provider_name: String,
    pub five_hour: UsageWindow,
    pub seven_day: UsageWindow,
    pub fetched_at_ms: i64,
    pub cached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<UsageMetadata>,
}

/// How long we serve a cached usage report before hitting the API again.
const CACHE_TTL_MS: i64 = 5 * 60 * 1_000;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Serve a stale cached report if available, otherwise return `err` as-is.
fn stale_report_or(
    read_cache: &impl Fn() -> Option<UsageReport>,
    err: AppError,
) -> Result<UsageReport, AppError> {
    if let Some(cached) = read_cache() {
        Ok(UsageReport { cached: true, ..cached })
    } else {
        Err(err)
    }
}

// ---------------------------------------------------------------------------
// Shared generate_report flow — every provider's generate_report delegates
// to this so the lifecycle logic cannot drift.
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn run_report_flow<S, F, Fut, Stats>(
    provider: &str,
    tokens: &TokenManager<S>,
    read_cache: impl Fn() -> Option<UsageReport>,
    write_cache: impl Fn(&UsageReport),
    fetch: F,
    map_stats: impl Fn(Stats) -> UsageReport,
    reauth_msg: &str,
    force_refresh: bool,
) -> Result<UsageReport, AppError>
where
    S: CredentialSource,
    F: Fn(CachedCredentials) -> Fut,
    Fut: Future<Output = Result<Stats, AppError>>,
{
    if !force_refresh {
        if let Some(cached) = read_cache() {
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
    let creds = match tokens.get_valid_token().await {
        Ok(c) => c,
        Err(AppError::Internal(msg)) => {
            eprintln!(
                "[timing] {provider}::generate_report (total, cached fallback) = {:?}",
                start.elapsed()
            );
            return stale_report_or(
                &read_cache,
                AppError::Internal(format!(
                    "credentials unavailable and no cached report available: {msg}"
                )),
            );
        }
        Err(e) => return Err(e),
    };
    eprintln!(
        "[timing] {provider}::generate_report get_valid_token = {:?}",
        _t1.elapsed()
    );

    let t2 = Instant::now();
    let result = fetch(creds.clone()).await;
    eprintln!(
        "[timing] {provider}::generate_report get_usage_stats = {:?}",
        t2.elapsed()
    );

    let rejected_token = creds.token.clone();

    match result {
        Ok(stats) => {
            let t3 = Instant::now();
            let report = map_stats(stats);
            write_cache(&report);
            eprintln!(
                "[timing] {provider}::generate_report write_cache = {:?}",
                t3.elapsed()
            );
            eprintln!(
                "[timing] {provider}::generate_report (total, success) = {:?}",
                start.elapsed()
            );
            Ok(report)
        }

        Err(AppError::Unauthorized(_)) => {
            let t3 = Instant::now();
            let creds = match tokens.recover_from_rejection(&rejected_token).await {
                Ok(c) => c,
                Err(AppError::ReauthRequired(msg)) => {
                    eprintln!(
                        "[timing] {provider}::generate_report (total, reauth) = {:?}",
                        start.elapsed()
                    );
                    return Err(AppError::ReauthRequired(msg));
                }
                Err(AppError::Internal(msg)) => {
                    eprintln!(
                        "[timing] {provider}::generate_report (total, cached fallback) = {:?}",
                        start.elapsed()
                    );
                    return stale_report_or(
                        &read_cache,
                        AppError::Internal(format!(
                            "credentials unavailable and no cached report available: {msg}"
                        )),
                    );
                }
                Err(e) => return Err(e),
            };
            eprintln!(
                "[timing] {provider}::generate_report recover = {:?}",
                t3.elapsed()
            );

            match fetch(creds).await {
                Ok(stats) => {
                    let report = map_stats(stats);
                    write_cache(&report);
                    eprintln!(
                        "[timing] {provider}::generate_report (total, retry ok) = {:?}",
                        start.elapsed()
                    );
                    Ok(report)
                }
                Err(AppError::Unauthorized(_)) => {
                    eprintln!(
                        "[timing] {provider}::generate_report (total, reauth) = {:?}",
                        start.elapsed()
                    );
                    Err(AppError::ReauthRequired(reauth_msg.into()))
                }
                Err(e) => {
                    if let Some(cached) = read_cache() {
                        eprintln!(
                            "[timing] {provider}::generate_report (total, retry cached fallback) = {:?}",
                            start.elapsed()
                        );
                        Ok(UsageReport { cached: true, ..cached })
                    } else {
                        Err(e)
                    }
                }
            }
        }

        Err(AppError::Internal(msg)) => {
            eprintln!(
                "[timing] {provider}::generate_report (total, cached fallback) = {:?}",
                start.elapsed()
            );
            stale_report_or(
                &read_cache,
                AppError::Internal(format!(
                    "API unavailable and no cached report available: {msg}"
                )),
            )
        }

        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Connector trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait UsageConnector: Send + Sync {
    fn provider_name(&self) -> &'static str;
    async fn generate_report(&self, force_refresh: bool) -> Result<UsageReport, AppError>;
}

// ---------------------------------------------------------------------------
// Facade / Manager
// ---------------------------------------------------------------------------

pub struct UsageManager {
    connectors: HashMap<&'static str, Box<dyn UsageConnector>>,
}

impl UsageManager {
    pub fn new(app: tauri::AppHandle) -> Result<Self, AppError> {
        let mut connectors: HashMap<&'static str, Box<dyn UsageConnector>> = HashMap::new();

        let codex = codex::CodexConnector::new(app.clone())?;
        connectors.insert(codex.provider_name(), Box::new(codex));

        let claude = claude::ClaudeConnector::new(app.clone())?;
        connectors.insert(claude.provider_name(), Box::new(claude));

        let ag = antigravity::AntigravityConnector::new(app)?;
        connectors.insert(ag.provider_name(), Box::new(ag));

        Ok(Self { connectors })
    }

    pub async fn generate_report(
        &self,
        provider: &str,
        force_refresh: bool,
    ) -> Result<UsageReport, AppError> {
        self.connectors
            .get(provider)
            .ok_or_else(|| AppError::NotFound(format!("unknown provider: {provider}")))?
            .generate_report(force_refresh)
            .await
    }
}

// ---------------------------------------------------------------------------
// IPC command
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_usage_report(
    provider: String,
    force_refresh: bool,
    manager: tauri::State<'_, UsageManager>,
) -> Result<UsageReport, AppError> {
    manager.generate_report(&provider, force_refresh).await
}
