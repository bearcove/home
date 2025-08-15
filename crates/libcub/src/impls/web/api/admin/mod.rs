use crate::impls::{
    cub_req::CubReqImpl,
    reply::{FacetJson, IntoLegacyReply, LegacyReply},
};
use axum::{Router, http::StatusCode, routing::get};
use facet::Facet;

/// Returns admin-only routes
pub(crate) fn admin_routes() -> Router {
    Router::new()
        .route("/all-users", get(serve_all_users))
        .layer(axum::middleware::from_fn(
            |req: axum::http::Request<axum::body::Body>, next: axum::middleware::Next| async move {
                let tr = req.extensions().get::<CubReqImpl>();
                match tr {
                    None => axum::http::Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(axum::body::Body::from("Internal server error"))
                        .unwrap(),
                    Some(tr) => {
                        if tr.viewer.is_admin {
                            next.run(req).await
                        } else {
                            axum::http::Response::builder()
                                .status(StatusCode::FORBIDDEN)
                                .body(axum::body::Body::from("Shoo!"))
                                .unwrap()
                        }
                    }
                }
            },
        ))
}

#[derive(Facet)]
struct AllUsers {
    users: Vec<UserSummary>,
}

#[derive(Facet)]
struct UserSummary {
    id: String,
    name: String,
    tier: Option<String>,
}

async fn serve_all_users(_tr: CubReqImpl) -> LegacyReply {
    let allusers = _tr.tenant.users.read().clone();
    FacetJson(allusers).into_legacy_reply()
}
