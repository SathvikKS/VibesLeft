use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    NotFound(String),
    Unauthorized(String),
    ReauthRequired(String),
    Internal(String),
}

impl std::error::Error for AppError {}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(m)
            | Self::Unauthorized(m)
            | Self::ReauthRequired(m)
            | Self::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(format!("I/O error: {e}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(format!("JSON error: {e}"))
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.status() == Some(reqwest::StatusCode::UNAUTHORIZED)
            || e.status() == Some(reqwest::StatusCode::FORBIDDEN)
        {
            AppError::Unauthorized("API rejected the request".into())
        } else {
            AppError::Internal(format!("HTTP request failed: {e}"))
        }
    }
}
