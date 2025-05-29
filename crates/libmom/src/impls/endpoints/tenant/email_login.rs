use axum::{extract::Json, http::StatusCode};
use credentials::{AuthBundle, UserInfo, Profile};
use time::{Duration, OffsetDateTime};
use mom_types::{
    GenerateLoginCodeRequest, GenerateLoginCodeResponse,
    ValidateLoginCodeRequest, ValidateLoginCodeResponse,
};
use config_types::is_development;

use crate::impls::site::{HttpError, IntoReply, Reply, FacetJson};
use super::super::tenant_extractor::TenantExtractor;

pub async fn generate_login_code(
    axum::Extension(TenantExtractor(ts)): axum::Extension<TenantExtractor>,
    Json(req): Json<GenerateLoginCodeRequest>,
) -> Reply {
    log::info!("Email login request received for email: {} (tenant: {})", req.email, ts.ti.tc.name);
    
    // Validate email format
    if !req.email.contains('@') || req.email.len() < 3 {
        log::warn!("Invalid email format received: {}", req.email);
        return HttpError::with_status(StatusCode::BAD_REQUEST, "Invalid email format").into_reply();
    }
    log::debug!("Email validation passed for: {}", req.email);

    // Generate a 6-digit numeric code using time and hash
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    // Create a better seed by combining timestamp with email
    let mut hasher = DefaultHasher::new();
    timestamp.hash(&mut hasher);
    req.email.hash(&mut hasher);
    let hash_value = hasher.finish();
    
    // Generate code from the hash, ensuring 6 digits
    let code_num = (hash_value % 900_000) + 100_000;
    let code = format!("{:06}", code_num);
    
    // Generate unique ID for this login attempt
    let id = format!("email-login-{}", timestamp);
    log::debug!("Generated login attempt ID: {}", id);
    
    // Set expiration to 15 minutes from now
    let created_at = OffsetDateTime::now_utc();
    let expires_at = created_at + Duration::minutes(15);
    log::debug!("Login code created at {} and expires at {}", created_at, expires_at);

    // Store in database
    log::debug!("Storing login code in database for email: {}", req.email);
    let conn = ts.pool.get()?;
    let rows_affected = conn.execute(
        "INSERT INTO email_login_codes (id, email, code, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![&id, &req.email, &code, &created_at, &expires_at],
    )?;
    log::debug!("Inserted {} row(s) into email_login_codes table", rows_affected);

    // Send email with code if email service is configured
    if let Some(email_service) = &crate::impls::global_state().email_service {
        log::info!("Email service is configured, attempting to send login code");
        match email_service.send_login_code(&req.email, &code, ts.ti.tc.name.as_str()).await {
            Ok(_) => {
                log::info!("Successfully sent login code to email: {}", req.email);
            }
            Err(e) => {
                log::error!("Failed to send login email to {}: {}", req.email, e);
                log::debug!("Email send error details: {:?}", e);
                // Continue anyway - we'll still return the code for development
                log::warn!("Continuing despite email send failure - code will be returned in response");
            }
        }
    } else {
        log::warn!("Email service not configured, cannot send login code to {}", req.email);
        if is_development() {
            log::info!("Development mode - login code for {}: {}", req.email, code);
        }
    }

    log::info!("Login code generation completed for email: {} (code expires at: {})", req.email, expires_at);
    FacetJson(GenerateLoginCodeResponse {
        code,
        expires_at,
    })
    .into_reply()
}

pub async fn validate_login_code(
    axum::Extension(TenantExtractor(ts)): axum::Extension<TenantExtractor>,
    Json(req): Json<ValidateLoginCodeRequest>,
) -> Reply {
    log::info!("Login code validation request for email: {} (tenant: {})", req.email, ts.ti.tc.name);
    log::debug!("Validating code: {} from IP: {:?}, User-Agent: {:?}", req.code, req.ip_address, req.user_agent);
    
    let conn = ts.pool.get()?;
    
    // Find the login code entry
    log::debug!("Looking up login code in database for email: {}", req.email);
    let (id, expires_at): (String, OffsetDateTime) = conn
        .query_row(
            "SELECT id, expires_at FROM email_login_codes WHERE email = ?1 AND code = ?2 AND used_at IS NULL",
            [&req.email, &req.code],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| {
            log::warn!("Failed to find valid login code for email {}: {:?}", req.email, e);
            HttpError::with_status(StatusCode::UNAUTHORIZED, "Invalid code or email")
        })?;
    log::debug!("Found login code entry with ID: {} (expires at: {})", id, expires_at);

    // Check if code has expired
    let now = OffsetDateTime::now_utc();
    if expires_at < now {
        log::warn!("Login code has expired for email {} (expired at: {}, current time: {})", req.email, expires_at, now);
        return HttpError::with_status(StatusCode::UNAUTHORIZED, "Code has expired").into_reply();
    }
    log::debug!("Login code is still valid (expires in: {:?})", expires_at - now);

    // Mark code as used
    log::debug!("Marking login code as used for ID: {}", id);
    let rows_updated = conn.execute(
        "UPDATE email_login_codes SET used_at = ?1, ip_address = ?2, user_agent = ?3 WHERE id = ?4",
        rusqlite::params![
            &OffsetDateTime::now_utc(),
            &req.ip_address,
            &req.user_agent,
            &id
        ],
    )?;
    log::debug!("Updated {} row(s) in email_login_codes table", rows_updated);

    // First check Stripe for subscription information
    log::info!("Looking up user subscription in Stripe for email: {}", req.email);
    let client = crate::impls::global_state().client.clone();
    let stripe_user_info = libstripe::load()
        .lookup_user_by_email(&ts.ti.tc, client, &req.email)
        .await
        .unwrap_or_else(|e| {
            log::error!("Failed to lookup user in Stripe for email {}: {}", req.email, e);
            log::debug!("Stripe lookup error details: {:?}", e);
            None
        });
    
    // Create auth bundle
    let auth_bundle = if let Some(user_info) = stripe_user_info {
        // User found in Stripe with subscription
        log::info!("User {} found in Stripe with active subscription", req.email);
        log::debug!("Stripe user info: {:?}", user_info);
        AuthBundle {
            expires_at: OffsetDateTime::now_utc() + Duration::days(30),
            user_info,
        }
    } else {
        // No Stripe subscription found, create basic auth
        log::info!("No Stripe subscription found for {}, creating basic auth", req.email);
        let email_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            req.email.to_lowercase().trim().hash(&mut hasher);
            format!("{:x}", hasher.finish())
        };
        
        AuthBundle {
            expires_at: OffsetDateTime::now_utc() + Duration::days(30),
            user_info: UserInfo {
                profile: Profile {
                    full_name: req.email.clone(),
                    patreon_id: None,
                    github_id: None,
                    email: Some(req.email.clone()),
                    thumb_url: format!("https://www.gravatar.com/avatar/{}?d=identicon&s=200", email_hash),
                },
                tier: None,
            },
        }
    };

    log::info!("Login code validation successful for email: {} (auth expires at: {})", req.email, auth_bundle.expires_at);
    FacetJson(ValidateLoginCodeResponse { auth_bundle }).into_reply()
}