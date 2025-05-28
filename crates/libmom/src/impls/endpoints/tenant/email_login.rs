use axum::{extract::Json, http::StatusCode};
use credentials::{AuthBundle, UserInfo, Profile};
use time::{Duration, OffsetDateTime};
use mom_types::{
    GenerateLoginCodeRequest, GenerateLoginCodeResponse,
    ValidateLoginCodeRequest, ValidateLoginCodeResponse,
};

use crate::impls::site::{HttpError, IntoReply, Reply, FacetJson};
use super::super::tenant_extractor::TenantExtractor;

pub async fn generate_login_code(
    axum::Extension(TenantExtractor(ts)): axum::Extension<TenantExtractor>,
    Json(req): Json<GenerateLoginCodeRequest>,
) -> Reply {
    // Validate email format
    if !req.email.contains('@') || req.email.len() < 3 {
        return HttpError::with_status(StatusCode::BAD_REQUEST, "Invalid email format").into_reply();
    }

    // Generate a 6-digit numeric code using time-based randomness
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let code = format!("{:06}", (timestamp % 1_000_000) as u32);
    
    // Generate unique ID for this login attempt
    let id = format!("email-login-{}", timestamp);
    
    // Set expiration to 15 minutes from now
    let created_at = OffsetDateTime::now_utc();
    let expires_at = created_at + Duration::minutes(15);

    // Store in database
    let conn = ts.pool.get()?;
    conn.execute(
        "INSERT INTO email_login_codes (id, email, code, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![&id, &req.email, &code, &created_at, &expires_at],
    )?;

    // Send email with code if email service is configured
    if let Some(email_service) = &crate::impls::global_state().email_service {
        match email_service.send_login_code(&req.email, &code, ts.ti.tc.name.as_str()).await {
            Ok(_) => {
                log::info!("Sent login code to email {}", req.email);
            }
            Err(e) => {
                log::error!("Failed to send login email: {e}");
                // Continue anyway - we'll still return the code for development
            }
        }
    } else {
        log::warn!("Email service not configured, cannot send login code to {}", req.email);
    }

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
    let conn = ts.pool.get()?;
    
    // Find the login code entry
    let (id, expires_at): (String, OffsetDateTime) = conn
        .query_row(
            "SELECT id, expires_at FROM email_login_codes WHERE email = ?1 AND code = ?2 AND used_at IS NULL",
            [&req.email, &req.code],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| {
            HttpError::with_status(StatusCode::UNAUTHORIZED, "Invalid code or email")
        })?;

    // Check if code has expired
    if expires_at < OffsetDateTime::now_utc() {
        return HttpError::with_status(StatusCode::UNAUTHORIZED, "Code has expired").into_reply();
    }

    // Mark code as used
    conn.execute(
        "UPDATE email_login_codes SET used_at = ?1, ip_address = ?2, user_agent = ?3 WHERE id = ?4",
        rusqlite::params![
            &OffsetDateTime::now_utc(),
            &req.ip_address,
            &req.user_agent,
            &id
        ],
    )?;

    // First check Stripe for subscription information
    let client = crate::impls::global_state().client.clone();
    let stripe_user_info = libstripe::load()
        .lookup_user_by_email(&ts.ti.tc, client, &req.email)
        .await
        .unwrap_or_else(|e| {
            log::error!("Failed to lookup user in Stripe: {e}");
            None
        });
    
    // Create auth bundle
    let auth_bundle = if let Some(user_info) = stripe_user_info {
        // User found in Stripe with subscription
        AuthBundle {
            expires_at: OffsetDateTime::now_utc() + Duration::days(30),
            user_info,
        }
    } else {
        // No Stripe subscription found, create basic auth
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
                    thumb_url: format!("https://www.gravatar.com/avatar/{}?d=identicon&s=200", email_hash),
                },
                tier: None,
            },
        }
    };

    FacetJson(ValidateLoginCodeResponse { auth_bundle }).into_reply()
}