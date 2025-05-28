use crate::impls::reply::{IntoLegacyReply, LegacyHttpError, LegacyReply};
use axum::{
    Router, http,
    routing::{get, post},
};
use http::StatusCode;

mod autocomplete;
mod comments;
mod git_token;
mod git_token_get;
mod link_preview;
mod update_userinfo;

/// Returns routes that are available in both development and production
pub(crate) fn public_api_routes() -> Router {
    Router::new()
        .route("/comments", get(comments::serve_comments))
        .route("/autocomplete", get(autocomplete::serve_autocomplete))
        .route(
            "/update-userinfo",
            post(update_userinfo::serve_update_userinfo),
        )
        .route("/link-preview", get(link_preview::serve_link_preview))
        .route("/git-token", get(git_token_get::serve_git_token_info).post(git_token::serve_git_token))
        .route("/{*splat}", get(serve_api_not_found).post(serve_api_not_found))
}

async fn serve_api_not_found() -> LegacyReply {
    LegacyHttpError::with_status(StatusCode::NOT_FOUND, "API endpoint not found")
        .into_legacy_reply()
}
