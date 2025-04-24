use crate::impls::{
    cub_req::CubReqImpl,
    reply::{IntoLegacyReply, LegacyReply},
};
use axum::{Router, routing::get};

use super::h_to_axum;

pub(crate) fn routes() -> Router {
    Router::new()
        .route("/", get(serve_root))
        .route("/*path", get(serve_asset))
}

async fn serve_root() -> LegacyReply {
    "you found my CDN! (it's written in Rust!)".into_legacy_reply()
}

async fn serve_asset(crx: CubReqImpl, headers: axum::http::HeaderMap) -> LegacyReply {
    h_to_axum(libcdn::load().serve_asset(Box::new(crx), headers).await)
}
