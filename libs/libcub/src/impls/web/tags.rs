use crate::impls::{
    cub_req::{CubReqImpl, RenderArgs},
    reply::{IntoLegacyReply, LegacyReply},
};
use axum::{extract::Path, response::Redirect};
use content_type::ContentType;

use axum::{routing::get, Router};

pub(crate) fn tag_routes() -> Router<()> {
    Router::new()
        .route("/", get(serve_list))
        .route("/:tag", get(serve_single))
        .route("/:tag/", get(redirect_to_slashless))
}

async fn serve_list(tr: CubReqImpl) -> LegacyReply {
    tr.render(RenderArgs::new("tags.html"))
}

async fn serve_single(tr: CubReqImpl, Path(tag): Path<String>) -> LegacyReply {
    tr.render(
        RenderArgs::new("tag.html")
            .with_global("tag", tag)
            .with_content_type(ContentType::HTML),
    )
}

async fn redirect_to_slashless(Path(tag): Path<String>) -> LegacyReply {
    Redirect::permanent(&format!("/tags/{tag}")).into_legacy_reply()
}
