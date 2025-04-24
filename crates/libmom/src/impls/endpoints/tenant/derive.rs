use axum::Extension;

use super::TenantExtractor;
use crate::impls::site::Reply;
use mom_types::DeriveParams;

pub(crate) async fn derive(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: String,
) -> Reply {
    let params: DeriveParams = merde::json::from_str(&body).unwrap();

    // spawn, because the task isn't cancelled even if the request drops — started derivations are
    // finished.
    tokio::spawn(crate::impls::deriver::do_derive(ts, params))
        .await
        .unwrap()
}
