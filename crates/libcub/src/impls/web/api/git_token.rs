use crate::impls::{
    cub_req::CubReqImpl,
    git_auth::generate_git_clone_token,
    reply::{LegacyHttpError, LegacyReply},
};
use axum::{Json, response::IntoResponse};
use cub_types::{CubReq, CubTenant};
use http::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct GitTokenResponse {
    token: String,
    clone_url: String,
    expires_in: u64,
}

#[derive(Deserialize)]
pub struct GitTokenRequest {
    /// Repository path (e.g., "my-repo")
    repo: String,
}

pub async fn serve_git_token(tr: CubReqImpl, Json(req): Json<GitTokenRequest>) -> LegacyReply {
    log::info!("serve_git_token: Processing request for repo: {}", req.repo);
    
    // Check if auth_bundle exists
    if tr.auth_bundle.is_none() {
        log::warn!("serve_git_token: No auth_bundle present in request");
        return Err(LegacyHttpError::with_status(
            StatusCode::UNAUTHORIZED, 
            "Authentication required"
        ));
    }
    
    // Validate user is authenticated
    let viewer = tr.viewer()?;
    log::info!("serve_git_token: Viewer status - is_admin: {}, has_bronze: {}, has_silver: {}", 
        viewer.is_admin, viewer.has_bronze, viewer.has_silver);
    
    if !viewer.has_bronze {
        log::warn!("serve_git_token: User does not have Bronze tier");
        return Err(LegacyHttpError::with_status(
            StatusCode::FORBIDDEN,
            "Git access requires Bronze tier or higher",
        ));
    }

    // Get the auth bundle to access user profile
    let auth_bundle = tr.auth_bundle.as_ref().unwrap();
    log::info!("serve_git_token: Auth bundle present, getting global_id");
    
    let global_id = auth_bundle.user_info.profile.global_id().map_err(|e| {
        log::error!("serve_git_token: Failed to get global_id: {}", e);
        log::error!("serve_git_token: Profile details - patreon_id: {:?}, github_id: {:?}, email: {:?}", 
            auth_bundle.user_info.profile.patreon_id,
            auth_bundle.user_info.profile.github_id,
            auth_bundle.user_info.profile.email
        );
        LegacyHttpError::with_status(StatusCode::UNAUTHORIZED, "No valid authentication found")
    })?;

    log::info!("serve_git_token: Got global_id: {}", global_id);
    
    // Generate JWT token with 31 day expiration
    let duration_secs = 31 * 24 * 60 * 60;
    let cookie_sauce = tr.tenant.tc().cookie_sauce();
    let token = generate_git_clone_token(global_id, &cookie_sauce, duration_secs).map_err(|e| {
        log::error!("Failed to generate git token: {e}");
        LegacyHttpError::with_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to generate token",
        )
    })?;

    // Construct clone URL
    let web_base_url = tr.tenant.tc().web_base_url(tr.web());
    let clone_url = format!("{}/extras/{}.git", web_base_url, req.repo);

    log::info!("serve_git_token: Successfully generated token for repo: {}", req.repo);
    
    Ok(Json(GitTokenResponse {
        token,
        clone_url,
        expires_in: duration_secs,
    })
    .into_response())
}
