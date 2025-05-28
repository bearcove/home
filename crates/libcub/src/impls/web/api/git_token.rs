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
    // Validate user is authenticated
    let viewer = tr.viewer()?;
    if !viewer.has_bronze {
        return Err(LegacyHttpError::with_status(
            StatusCode::FORBIDDEN,
            "Git access requires Bronze tier or higher",
        ));
    }

    // Get the auth bundle to access user profile
    let auth_bundle = tr.auth_bundle.as_ref().ok_or_else(|| {
        LegacyHttpError::with_status(StatusCode::UNAUTHORIZED, "Authentication required")
    })?;
    let global_id = auth_bundle.user_info.profile.global_id().map_err(|_| {
        LegacyHttpError::with_status(StatusCode::UNAUTHORIZED, "No valid authentication found")
    })?;

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

    Ok(Json(GitTokenResponse {
        token,
        clone_url,
        expires_in: duration_secs,
    })
    .into_response())
}
