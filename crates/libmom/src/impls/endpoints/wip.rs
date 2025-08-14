use axum::http::StatusCode;
use config_types::is_development;

use crate::impls::{
    endpoints::tenant_extractor::TenantExtractor,
    global_state,
    site::{FacetJson, IntoReply, Reply},
};

#[axum::debug_handler]
pub(crate) async fn serve_wip(TenantExtractor(ts): TenantExtractor) -> Reply {
    if !is_development() {
        return (StatusCode::NOT_FOUND, "Not found").into_reply();
    }

    let discord = libdiscord::load();
    let client = global_state().client.as_ref();

    let tc = &ts.ti.tc;

    log::info!("Listing bot guilds...");
    let guilds = discord.list_bot_guilds(tc, client).await?;

    FacetJson(guilds).into_reply()
}
