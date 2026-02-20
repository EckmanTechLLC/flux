//! OAuth token exchange logic.
//!
//! Handles exchanging authorization codes for access tokens.

use crate::credentials::Credentials;
use anyhow::{anyhow, Context, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OAuth token exchange request
#[derive(Serialize)]
struct TokenRequest {
    grant_type: String,
    code: String,
    redirect_uri: String,
    client_id: String,
    client_secret: String,
}

/// OAuth token response (standard OAuth 2.0)
#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    token_type: Option<String>,
}

/// Exchange authorization code for access token
///
/// # Arguments
/// * `token_url` - OAuth token endpoint URL
/// * `code` - Authorization code from callback
/// * `redirect_uri` - Redirect URI used in authorization request
/// * `client_id` - OAuth client ID
/// * `client_secret` - OAuth client secret
///
/// # Returns
/// * `Ok(Credentials)` - Access token, refresh token, and expiration
/// * `Err` - If token exchange fails
pub async fn exchange_code_for_token(
    token_url: &str,
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<Credentials> {
    let client = reqwest::Client::new();

    // Build form data for token exchange
    let mut form_data = HashMap::new();
    form_data.insert("grant_type", "authorization_code");
    form_data.insert("code", code);
    form_data.insert("redirect_uri", redirect_uri);
    form_data.insert("client_id", client_id);
    form_data.insert("client_secret", client_secret);

    tracing::debug!("Exchanging authorization code for token at {}", token_url);

    // Make POST request to token endpoint
    let response = client
        .post(token_url)
        .header("Accept", "application/json")
        .form(&form_data)
        .send()
        .await
        .context("Failed to send token exchange request")?;

    // Check response status
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow!(
            "Token exchange failed with status {}: {}",
            status,
            body
        ));
    }

    // Parse token response
    let token_response: TokenResponse = response
        .json()
        .await
        .context("Failed to parse token response")?;

    tracing::debug!(
        "Token exchange successful, has_refresh_token={}, expires_in={:?}",
        token_response.refresh_token.is_some(),
        token_response.expires_in
    );

    // Calculate expiration time
    let expires_at = token_response.expires_in.map(|seconds| {
        Utc::now() + Duration::seconds(seconds)
    });

    Ok(Credentials {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a mock OAuth server or are integration tests
    // For unit testing, we'd need to mock reqwest::Client

    #[test]
    fn test_token_response_deserialization() {
        // Test with all fields
        let json = r#"{
            "access_token": "gho_1234567890",
            "refresh_token": "ghr_0987654321",
            "expires_in": 3600,
            "token_type": "Bearer"
        }"#;

        let response: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.access_token, "gho_1234567890");
        assert_eq!(response.refresh_token, Some("ghr_0987654321".to_string()));
        assert_eq!(response.expires_in, Some(3600));
        assert_eq!(response.token_type, Some("Bearer".to_string()));
    }

    #[test]
    fn test_token_response_minimal() {
        // Test with only access_token (minimal response)
        let json = r#"{
            "access_token": "token_12345"
        }"#;

        let response: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.access_token, "token_12345");
        assert_eq!(response.refresh_token, None);
        assert_eq!(response.expires_in, None);
    }
}
