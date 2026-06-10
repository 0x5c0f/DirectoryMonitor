use axum::http::{HeaderMap, StatusCode};
use std::time::Instant;
use tracing::info;

use crate::server::{AppState, TOKEN_TTL_SECS};

/// Check if auth is required and token is valid.
/// Returns Ok(()) if access is allowed, Err(status) otherwise.
pub(crate) async fn check_auth(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode> {
    let password = state.config.read().await.server.password.clone();
    if password.is_empty() {
        return Ok(()); // No auth configured
    }

    let token = extract_token(headers);
    match token {
        Some(t) => {
            let tokens = state.tokens.read().await;
            if let Some(created) = tokens.get(&t) {
                if created.elapsed().as_secs() < TOKEN_TTL_SECS {
                    Ok(())
                } else {
                    Err(StatusCode::UNAUTHORIZED)
                }
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Extract bearer token from Authorization header.
pub(crate) fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Constant-time byte comparison to prevent timing attacks.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

/// GET /api/auth/status — check if authentication is required (no auth needed).
pub(crate) async fn auth_status_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Json<serde_json::Value> {
    let password = state.config.read().await.server.password.clone();
    axum::response::Json(serde_json::json!({
        "auth_required": !password.is_empty()
    }))
}

/// POST /api/auth/login — authenticate with password.
pub(crate) async fn auth_login_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    let password = state.config.read().await.server.password.clone();

    if password.is_empty() {
        return Ok(axum::response::Json(serde_json::json!({
            "ok": true,
            "token": null,
            "message": "No password configured"
        })));
    }

    let provided = body.get("password").and_then(|v| v.as_str()).unwrap_or("");

    // Constant-time comparison to prevent timing attacks
    if constant_time_eq(provided.as_bytes(), password.as_bytes()) {
        let token = uuid::Uuid::new_v4().to_string();
        state
            .tokens
            .write()
            .await
            .insert(token.clone(), Instant::now());
        info!("Auth: login successful");
        Ok(axum::response::Json(serde_json::json!({
            "ok": true,
            "token": token
        })))
    } else {
        info!("Auth: login failed");
        Err(StatusCode::FORBIDDEN)
    }
}

/// GET /api/auth/verify — check if token is still valid.
pub(crate) async fn auth_verify_handler(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    let password = state.config.read().await.server.password.clone();

    if password.is_empty() {
        return Ok(axum::response::Json(serde_json::json!({
            "ok": true,
            "auth_required": false
        })));
    }

    match extract_token(&headers) {
        Some(t) => {
            let tokens = state.tokens.read().await;
            if let Some(created) = tokens.get(&t) {
                if created.elapsed().as_secs() < TOKEN_TTL_SECS {
                    Ok(axum::response::Json(serde_json::json!({
                        "ok": true,
                        "auth_required": true
                    })))
                } else {
                    Err(StatusCode::UNAUTHORIZED)
                }
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_token_valid() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer my-secret-token-123"),
        );

        let token = extract_token(&headers);
        assert_eq!(token, Some("my-secret-token-123".to_string()));
    }

    #[test]
    fn test_extract_token_missing() {
        let headers = HeaderMap::new();
        let token = extract_token(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn test_extract_token_invalid_format() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );

        let token = extract_token(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn test_extract_token_empty_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer "));

        let token = extract_token(&headers);
        assert_eq!(token, Some("".to_string()));
    }

    #[test]
    fn test_extract_token_invalid_header_value() {
        let mut headers = HeaderMap::new();
        // Non-ASCII bytes are invalid header values
        headers.insert(
            "authorization",
            HeaderValue::from_bytes(&[0xFF, 0xFE]).unwrap(),
        );

        let token = extract_token(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn test_constant_time_eq_identical() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn test_constant_time_eq_different() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn test_constant_time_eq_different_length() {
        assert!(!constant_time_eq(b"hello", b"hi"));
    }

    #[test]
    fn test_constant_time_eq_empty() {
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_constant_time_eq_single_byte_diff() {
        // Ensure every bit position is checked
        assert!(!constant_time_eq(b"\x00", b"\x01"));
        assert!(!constant_time_eq(b"\x00", b"\x80"));
    }
}
