use crate::impls::reply::{IntoLegacyReply, LegacyHttpError, LegacyReply};
use axum::{
    Router, http,
    routing::{get, post},
};
use http::StatusCode;

mod admin;
mod autocomplete;
mod comments;
mod link_preview;
mod make_api_key;
mod refresh_userinfo;

/// Returns routes that are available in both development and production
pub(crate) fn public_api_routes() -> Router {
    Router::new()
        .nest("/admin", admin::admin_routes())
        .route("/comments", get(comments::serve_comments))
        .route("/autocomplete", get(autocomplete::serve_autocomplete))
        .route(
            "/refresh-userinfo",
            post(refresh_userinfo::serve_refresh_userinfo),
        )
        .route("/link-preview", get(link_preview::serve_link_preview))
        .route("/make-api-key", post(make_api_key::serve_make_api_key))
        .route(
            "/{*splat}",
            get(serve_api_not_found).post(serve_api_not_found),
        )
}

async fn serve_api_not_found() -> LegacyReply {
    LegacyHttpError::with_status(StatusCode::NOT_FOUND, "API endpoint not found")
        .into_legacy_reply()
}
