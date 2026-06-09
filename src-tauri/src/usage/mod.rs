use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

mod claude;
mod codex;
mod creds_cache;

pub(super) use creds_cache::{CachedCredentials, CredsCache};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageWindow {
    pub utilization: f64,
    pub resets_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageReport {
    pub provider_name: String,
    pub five_hour: UsageWindow,
    pub seven_day: UsageWindow,
    pub fetched_at_ms: i64,
    pub cached: bool,
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

        let claude = claude::ClaudeConnector::new(app)?;
        connectors.insert(claude.provider_name(), Box::new(claude));

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
