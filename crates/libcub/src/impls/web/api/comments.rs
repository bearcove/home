use crate::impls::{cub_req::CubReqImpl, h_to_axum, reply::LegacyReply};

pub(crate) async fn serve_comments(rcx: CubReqImpl) -> LegacyReply {
    h_to_axum(libapi::load().serve_comments(Box::new(rcx)).await)
}
