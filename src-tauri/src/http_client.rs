use crate::error::AppError;

pub fn build_client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(cfg!(debug_assertions))
        .build()
        .map_err(|e| AppError::Internal(format!("failed to build HTTP client: {e}")))
}
