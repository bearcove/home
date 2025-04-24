use axum::{
    Router,
    http::StatusCode,
    routing::{get, post},
};
use config_types::is_development;

use crate::impls::reply::{IntoLegacyReply, LegacyHttpError, LegacyReply};

mod deploy;
mod download_url;
mod edit_asset;
mod internal_search;
mod media_upload;
mod open_in_editor;
mod validation;
mod ws;

/// Returns routes that are only available in development mode
pub(crate) fn internal_api_routes() -> Router {
    Router::new()
        .layer(axum::middleware::from_fn(
            |req: axum::http::Request<axum::body::Body>, next: axum::middleware::Next| async move {
                if is_development() {
                    return next.run(req).await;
                }

                let path = req.uri().path();
                tracing::info!("Blocking access to {path} in production");
                axum::http::Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(axum::body::Body::empty())
                    .unwrap()
            },
        ))
        .route("/ws", get(ws::serve_ws))
        .route(
            "/open-in-editor",
            post(open_in_editor::serve_open_in_editor),
        )
        .route("/edit-asset", post(edit_asset::serve_edit_asset))
        .route("/deploy", get(deploy::serve))
        .route("/validation", get(validation::serve))
        .route("/media-upload", get(media_upload::serve_media_upload))
        .route("/search-assets", get(internal_search::search_assets))
        .route("/search-inputs", get(internal_search::search_inputs))
        .route("/download-url", get(download_url::download_url))
        .route("/builtins/ansi.css", get(ansi_css))
        .route("/builtins/livereload.js", get(livereload_js))
        .route("/*splat", get(serve_api_not_found))
}

async fn serve_api_not_found() -> LegacyReply {
    LegacyHttpError::with_status(StatusCode::NOT_FOUND, "API endpoint not found")
        .into_legacy_reply()
}

async fn livereload_js() -> LegacyReply {
    let code = include_str!("livereload.js");

    Ok(http::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/javascript; charset=utf-8")
        .header("cache-control", "no-cache")
        .body(code.into())
        .unwrap())
}

async fn ansi_css() -> LegacyReply {
    let code = libterm::load().css();

    Ok(http::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/css; charset=utf-8")
        .header("cache-control", "no-cache")
        .body(code.into())
        .unwrap())
}
