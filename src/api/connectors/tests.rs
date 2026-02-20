//! Unit tests for connector status API

use super::*;

#[test]
fn test_connector_summary_serialization() {
    let summary = ConnectorSummary {
        name: "github".to_string(),
        enabled: true,
        status: "configured".to_string(),
    };

    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("\"name\":\"github\""));
    assert!(json.contains("\"enabled\":true"));
    assert!(json.contains("\"status\":\"configured\""));
}

#[test]
fn test_connector_detail_serialization() {
    let detail = ConnectorDetail {
        name: "github".to_string(),
        enabled: true,
        status: "active".to_string(),
        last_poll: Some("2026-02-17T10:30:00Z".to_string()),
        last_error: None,
        poll_interval_seconds: 300,
    };

    let json = serde_json::to_string(&detail).unwrap();
    assert!(json.contains("\"name\":\"github\""));
    assert!(json.contains("\"enabled\":true"));
    assert!(json.contains("\"status\":\"active\""));
    assert!(json.contains("\"last_poll\":\"2026-02-17T10:30:00Z\""));
    assert!(!json.contains("\"last_error\"")); // Should be omitted when None
    assert!(json.contains("\"poll_interval_seconds\":300"));
}

#[test]
fn test_connector_detail_optional_fields() {
    let detail = ConnectorDetail {
        name: "gmail".to_string(),
        enabled: false,
        status: "not_configured".to_string(),
        last_poll: None,
        last_error: None,
        poll_interval_seconds: 60,
    };

    let json = serde_json::to_string(&detail).unwrap();
    // Optional fields should not appear when None
    assert!(!json.contains("\"last_poll\""));
    assert!(!json.contains("\"last_error\""));
}

#[test]
fn test_list_connectors_response_serialization() {
    let response = ListConnectorsResponse {
        connectors: vec![
            ConnectorSummary {
                name: "github".to_string(),
                enabled: true,
                status: "configured".to_string(),
            },
            ConnectorSummary {
                name: "gmail".to_string(),
                enabled: false,
                status: "not_configured".to_string(),
            },
        ],
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"connectors\""));
    assert!(json.contains("\"github\""));
    assert!(json.contains("\"gmail\""));
}

#[test]
fn test_available_connectors_list() {
    // Verify expected connectors from ADR-005
    assert_eq!(AVAILABLE_CONNECTORS.len(), 4);
    assert!(AVAILABLE_CONNECTORS.contains(&"github"));
    assert!(AVAILABLE_CONNECTORS.contains(&"gmail"));
    assert!(AVAILABLE_CONNECTORS.contains(&"linkedin"));
    assert!(AVAILABLE_CONNECTORS.contains(&"calendar"));
}

#[test]
fn test_token_request_deserialization() {
    let json = r#"{"token":"ghp_abc123"}"#;
    let req: TokenRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.token, "ghp_abc123");
}

#[test]
fn test_store_token_response_serialization() {
    let resp = StoreTokenResponse { success: true };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"success\":true"));

    let resp_fail = StoreTokenResponse { success: false };
    let json_fail = serde_json::to_string(&resp_fail).unwrap();
    assert!(json_fail.contains("\"success\":false"));
}

#[test]
fn test_delete_token_response_serialization() {
    let resp = DeleteTokenResponse { success: true };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"success\":true"));
}
