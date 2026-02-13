use axum::http::HeaderMap;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// Extract bearer token from HTTP Authorization header
///
/// Expected format: "Authorization: Bearer <token>"
/// Returns the token string if present and valid.
pub fn extract_bearer_token(headers: &HeaderMap) -> Result<String, TokenError> {
    // Get Authorization header
    let auth_header = headers
        .get("authorization")
        .ok_or(TokenError::Missing)?
        .to_str()
        .map_err(|_| TokenError::InvalidFormat)?;

    // Parse "Bearer <token>" format
    parse_bearer_token(auth_header)
}

/// Extract token from WebSocket JSON message
///
/// Expected format: {"token": "<uuid>", "type": "subscribe", ...}
/// Returns the token string if present and valid.
pub fn extract_token_from_message(message: &Value) -> Result<String, TokenError> {
    // Get "token" field from JSON
    let token = message
        .get("token")
        .ok_or(TokenError::Missing)?
        .as_str()
        .ok_or(TokenError::InvalidFormat)?;

    // Validate not empty
    if token.is_empty() {
        return Err(TokenError::Empty);
    }

    Ok(token.to_string())
}

/// Parse bearer token from Authorization header value
///
/// Internal helper for extract_bearer_token
fn parse_bearer_token(header_value: &str) -> Result<String, TokenError> {
    // Expect "Bearer <token>"
    let parts: Vec<&str> = header_value.splitn(2, ' ').collect();

    if parts.len() != 2 {
        return Err(TokenError::InvalidFormat);
    }

    // Check scheme is "Bearer"
    if parts[0].to_lowercase() != "bearer" {
        return Err(TokenError::InvalidFormat);
    }

    // Get token part
    let token = parts[1].trim();

    // Validate not empty
    if token.is_empty() {
        return Err(TokenError::Empty);
    }

    Ok(token.to_string())
}

/// Token extraction errors
#[derive(Debug, PartialEq, Clone)]
pub enum TokenError {
    /// Authorization header or token field not present
    Missing,
    /// Invalid format (not "Bearer <token>" or non-string token)
    InvalidFormat,
    /// Token is empty string
    Empty,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Missing => write!(f, "Authorization token not provided"),
            TokenError::InvalidFormat => write!(f, "Invalid authorization token format"),
            TokenError::Empty => write!(f, "Authorization token is empty"),
        }
    }
}

impl std::error::Error for TokenError {}
