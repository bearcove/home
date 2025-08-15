use crate::impls::{
    cub_req::CubReqImpl,
    reply::{FacetJson, IntoLegacyReply, LegacyReply},
};
use axum::{
    Router,
    body::Body,
    http::StatusCode,
    routing::{get, post},
};
use cub_types::CubTenant;

/// Returns admin-only routes
pub(crate) fn admin_routes() -> Router {
    Router::new()
        .route("/all-users", get(serve_all_users))
        .route("/opendoor", post(serve_opendoor))
        .layer(axum::middleware::from_fn(
            |req: axum::http::Request<Body>, next: axum::middleware::Next| async move {
                let tr = req.extensions().get::<CubReqImpl>();
                match tr {
                    None => axum::http::Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Internal server error"))
                        .unwrap(),
                    Some(tr) => {
                        if tr.viewer.is_admin {
                            next.run(req).await
                        } else {
                            axum::http::Response::builder()
                                .status(StatusCode::FORBIDDEN)
                                .body(Body::from("Shoo!"))
                                .unwrap()
                        }
                    }
                }
            },
        ))
}

async fn serve_all_users(_tr: CubReqImpl) -> LegacyReply {
    let allusers = _tr.tenant.users.read().clone();
    FacetJson(allusers).into_legacy_reply()
}

async fn serve_opendoor(_tr: CubReqImpl, body: Body) -> LegacyReply {
    log::info!("serve_opendoor: starting request");
    let tcli = _tr.tenant.tcli();
    let bytes = match axum::body::to_bytes(body, 4 * 1024 * 1024).await {
        Ok(bytes) => {
            log::info!("serve_opendoor: received {} bytes", bytes.len());
            bytes
        }
        Err(e) => {
            log::info!("serve_opendoor: failed to read request body: {e}");
            return Ok(axum::http::Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("Request body too large or invalid"))
                .unwrap());
        }
    };
    log::info!("serve_opendoor: calling tcli.opendoor");
    let response = tcli.opendoor(bytes).await?;
    let status = response.status();
    log::info!("serve_opendoor: received response with status {status}");
    let headers = response.headers_only_string_safe();
    let body = response.bytes().await?;
    log::info!(
        "serve_opendoor: received response body with {} bytes",
        body.len()
    );

    let mut response_builder = axum::http::Response::builder().status(status);

    for (key, value) in headers {
        response_builder = response_builder.header(key, value);
    }

    log::info!("serve_opendoor: returning response");
    Ok(response_builder.body(Body::from(body))?)
}
