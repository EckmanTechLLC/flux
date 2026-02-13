use super::*;
use axum::http::HeaderMap;
use serde_json::json;

#[cfg(test)]
mod extract_bearer_token_tests {
    use super::*;

    #[test]
    fn valid_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer 550e8400-e29b-41d4-a716-446655440000"
                .parse()
                .unwrap(),
        );

        let result = extract_bearer_token(&headers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn valid_bearer_token_with_extra_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer   550e8400-e29b-41d4-a716-446655440000  "
                .parse()
                .unwrap(),
        );

        let result = extract_bearer_token(&headers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn case_insensitive_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "bearer 550e8400-e29b-41d4-a716-446655440000"
                .parse()
                .unwrap(),
        );

        let result = extract_bearer_token(&headers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn missing_authorization_header() {
        let headers = HeaderMap::new();
        let result = extract_bearer_token(&headers);
        assert_eq!(result, Err(TokenError::Missing));
    }

    #[test]
    fn empty_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "".parse().unwrap());

        let result = extract_bearer_token(&headers);
        assert_eq!(result, Err(TokenError::InvalidFormat));
    }

    #[test]
    fn missing_bearer_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "550e8400-e29b-41d4-a716-446655440000".parse().unwrap(),
        );

        let result = extract_bearer_token(&headers);
        assert_eq!(result, Err(TokenError::InvalidFormat));
    }

    #[test]
    fn wrong_auth_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Basic dXNlcjpwYXNz".parse().unwrap(),
        );

        let result = extract_bearer_token(&headers);
        assert_eq!(result, Err(TokenError::InvalidFormat));
    }

    #[test]
    fn bearer_without_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer".parse().unwrap());

        let result = extract_bearer_token(&headers);
        assert_eq!(result, Err(TokenError::InvalidFormat));
    }

    #[test]
    fn bearer_with_empty_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer  ".parse().unwrap());

        let result = extract_bearer_token(&headers);
        assert_eq!(result, Err(TokenError::Empty));
    }
}

#[cfg(test)]
mod extract_token_from_message_tests {
    use super::*;

    #[test]
    fn valid_token_in_message() {
        let message = json!({
            "type": "subscribe",
            "token": "550e8400-e29b-41d4-a716-446655440000",
            "entity_id": "sensor_42"
        });

        let result = extract_token_from_message(&message);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn token_as_first_field() {
        let message = json!({
            "token": "550e8400-e29b-41d4-a716-446655440000",
            "type": "subscribe",
            "entity_id": "sensor_42"
        });

        let result = extract_token_from_message(&message);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn missing_token_field() {
        let message = json!({
            "type": "subscribe",
            "entity_id": "sensor_42"
        });

        let result = extract_token_from_message(&message);
        assert_eq!(result, Err(TokenError::Missing));
    }

    #[test]
    fn empty_token() {
        let message = json!({
            "type": "subscribe",
            "token": "",
            "entity_id": "sensor_42"
        });

        let result = extract_token_from_message(&message);
        assert_eq!(result, Err(TokenError::Empty));
    }

    #[test]
    fn token_not_a_string() {
        let message = json!({
            "type": "subscribe",
            "token": 12345,
            "entity_id": "sensor_42"
        });

        let result = extract_token_from_message(&message);
        assert_eq!(result, Err(TokenError::InvalidFormat));
    }

    #[test]
    fn token_is_null() {
        let message = json!({
            "type": "subscribe",
            "token": null,
            "entity_id": "sensor_42"
        });

        let result = extract_token_from_message(&message);
        assert_eq!(result, Err(TokenError::InvalidFormat));
    }

    #[test]
    fn empty_message() {
        let message = json!({});

        let result = extract_token_from_message(&message);
        assert_eq!(result, Err(TokenError::Missing));
    }

    #[test]
    fn token_with_whitespace() {
        let message = json!({
            "type": "subscribe",
            "token": "  550e8400-e29b-41d4-a716-446655440000  ",
            "entity_id": "sensor_42"
        });

        // Should accept token as-is (with whitespace)
        let result = extract_token_from_message(&message);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "  550e8400-e29b-41d4-a716-446655440000  "
        );
    }
}

#[cfg(test)]
mod token_error_display_tests {
    use super::*;

    #[test]
    fn missing_error_message() {
        let error = TokenError::Missing;
        assert_eq!(error.to_string(), "Authorization token not provided");
    }

    #[test]
    fn invalid_format_error_message() {
        let error = TokenError::InvalidFormat;
        assert_eq!(error.to_string(), "Invalid authorization token format");
    }

    #[test]
    fn empty_error_message() {
        let error = TokenError::Empty;
        assert_eq!(error.to_string(), "Authorization token is empty");
    }
}
