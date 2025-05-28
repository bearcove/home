use base64::{engine::general_purpose::STANDARD, Engine};
use eyre::Result;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
pub struct GitCloneClaims {
    /// Subject - the user's global ID (e.g., "patreon:123" or "github:456")
    pub sub: String,
    /// Audience - always "git-clone" to prevent token reuse
    pub aud: String,
    /// Expiration time (as Unix timestamp)
    pub exp: u64,
    /// Issued at (as Unix timestamp)
    pub iat: u64,
}

impl GitCloneClaims {
    pub fn new(global_id: String, duration_secs: u64) -> Result<Self> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        
        Ok(Self {
            sub: global_id,
            aud: "git-clone".to_string(),
            iat: now,
            exp: now + duration_secs,
        })
    }
}

/// Generate a JWT token for git clone authentication
pub fn generate_git_clone_token(
    global_id: String,
    secret: &str,
    duration_secs: u64,
) -> Result<String> {
    let claims = GitCloneClaims::new(global_id, duration_secs)?;
    let header = Header::new(Algorithm::HS256);
    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

/// Validate and decode a git clone JWT token
pub fn validate_git_clone_token(token: &str, secret: &str) -> Result<GitCloneClaims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["git-clone"]);
    validation.set_required_spec_claims(&["sub", "aud", "exp", "iat"]);
    
    let token_data = decode::<GitCloneClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    
    Ok(token_data.claims)
}

/// Extract token from HTTP Basic Auth header
pub fn extract_token_from_basic_auth(auth_header: &str) -> Option<String> {
    // Basic auth format: "Basic base64(username:password)"
    let auth_header = auth_header.strip_prefix("Basic ")?;
    let decoded = STANDARD.decode(auth_header).ok()?;
    let credentials = String::from_utf8(decoded).ok()?;
    
    // We expect "token:JWT_TOKEN" format
    let parts: Vec<&str> = credentials.splitn(2, ':').collect();
    if parts.len() == 2 && parts[0] == "token" {
        Some(parts[1].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation_and_validation() {
        let secret = "test-secret-key";
        let global_id = "github:12345".to_string();
        let duration_secs = 3600; // 1 hour

        // Generate token
        let token = generate_git_clone_token(global_id.clone(), secret, duration_secs)
            .expect("Failed to generate token");

        // Validate token
        let claims = validate_git_clone_token(&token, secret)
            .expect("Failed to validate token");

        assert_eq!(claims.sub, global_id);
        assert_eq!(claims.aud, "git-clone");
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_extract_token_from_basic_auth() {
        let token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.test";
        let auth_value = format!("token:{}", token);
        let encoded = STANDARD.encode(&auth_value);
        let auth_header = format!("Basic {}", encoded);

        let extracted = extract_token_from_basic_auth(&auth_header);
        assert_eq!(extracted, Some(token.to_string()));
    }
}