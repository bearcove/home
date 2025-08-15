use axum::http::StatusCode;
use config_types::is_development;
use facet::Facet;
use libdiscord::{DiscordGuildMember, DiscordRole};
use sentrywrap::sentry;

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

        log::info!(
            "Listing channels for guild: {} ({})",
            first_guild.name,
            first_guild.id
        );
        let channels = discord
            .list_guild_channels(&first_guild.id, tc, client)
            .await?;

        // Try to find the "#bots" channel and send a message
        if let Some(bots_channel) = channels.iter().find(|c| c.name == "bots") {
            log::info!("Found #bots channel, sending message...");
            let _message = discord
                .post_message_to_channel(&bots_channel.id, "Wip ran!", tc, client)
                .await?;
        }

        #[derive(Facet)]
        struct Response {
            members: Vec<DiscordGuildMember>,
            roles: Vec<DiscordRole>,
            channels: Vec<libdiscord::DiscordChannel>,
        }

        let res = Response {
            members,
            roles,
            channels,
        };
        sentry::logger_info!(
            payload = facet_json::to_string(&res),
            "Fetched discord guild members, roles, and channels"
        );

        return FacetJson(res).into_reply();
    }

    (StatusCode::BAD_REQUEST, "Bot is not in any guilds").into_reply()
}
