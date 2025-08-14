use axum::http::StatusCode;
use config_types::is_development;
use facet::Facet;
use libdiscord::{DiscordGuildMember, DiscordRole};

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

    // Take the first guild and list its members
    if let Some(first_guild) = guilds.first() {
        log::info!(
            "Listing members for guild: {} ({})",
            first_guild.name,
            first_guild.id
        );
        let members = discord
            .list_guild_members(&first_guild.id, tc, client)
            .await?;

        log::info!(
            "Listing roles for guild: {} ({})",
            first_guild.name,
            first_guild.id
        );
        let roles = discord
            .list_guild_roles(&first_guild.id, tc, client)
            .await?;

        #[derive(Facet)]
        struct Response {
            members: Vec<DiscordGuildMember>,
            roles: Vec<DiscordRole>,
        }

        return FacetJson(Response { members, roles }).into_reply();
    }

    (StatusCode::BAD_REQUEST, "Bot is not in any guilds").into_reply()
}
