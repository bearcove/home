use std::collections::HashMap;

use cub_types::CubTenant;

use crate::impls::{
    cub_req::CubReqImpl,
    global_state::global_state,
    reply::{IntoLegacyReply, LegacyReply, MerdeJson},
};

pub(crate) async fn serve_autocomplete(
    query: axum::extract::Query<HashMap<String, String>>,
    tr: CubReqImpl,
) -> LegacyReply {
    let q = match query.get("q") {
        Some(value) => value,
        None => return http::StatusCode::BAD_REQUEST.into_legacy_reply(),
    };

    let irev = tr.tenant.rev()?;
    let index = tr.tenant.index()?;

    let results = index.autocomplete(irev.rev.as_ref(), &tr.viewer()?, q, global_state().web);

    MerdeJson(results).into_legacy_reply()
}
