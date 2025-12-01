use headers::{authorization::Basic, Authorization, HeaderMapExt};
use http::HeaderMap;

/// Credentials extracted from Basic Auth header
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Authentication extraction errors
#[derive(Debug)]
pub enum AuthError {
    /// No Authorization header present
    Missing,
    /// Authorization header present but invalid
    #[allow(dead_code)]
    Invalid,
}

/// Extract credentials from Basic Auth header using type-safe headers API
/// Returns error if no valid auth header is present
pub fn extract_basic_auth(headers: &HeaderMap) -> Result<Credentials, AuthError> {
    headers
        .typed_get::<Authorization<Basic>>()
        .ok_or(AuthError::Missing)
        .map(|auth| Credentials {
            username: auth.username().to_string(),
            password: auth.password().to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_basic_auth() {
        let mut headers = HeaderMap::new();
        // "user:pass" in base64
        headers.insert("authorization", "Basic dXNlcjpwYXNz".parse().unwrap());

        let creds = extract_basic_auth(&headers).unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass");
    }

    #[test]
    fn test_missing_auth_header() {
        let headers = HeaderMap::new();
        assert!(matches!(
            extract_basic_auth(&headers),
            Err(AuthError::Missing)
        ));
    }

    #[test]
    fn test_password_with_colon() {
        let mut headers = HeaderMap::new();
        // "user:pass:word" in base64
        headers.insert(
            "authorization",
            "Basic dXNlcjpwYXNzOndvcmQ=".parse().unwrap(),
        );

        let creds = extract_basic_auth(&headers).unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass:word");
    }

    #[test]
    fn test_empty_password() {
        let mut headers = HeaderMap::new();
        // "user:" in base64
        headers.insert("authorization", "Basic dXNlcjo=".parse().unwrap());

        let creds = extract_basic_auth(&headers).unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "");
    }

    #[test]
    fn test_invalid_base64() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Basic !!!invalid!!!".parse().unwrap());

        // This should fail to parse
        assert!(extract_basic_auth(&headers).is_err());
    }
}
