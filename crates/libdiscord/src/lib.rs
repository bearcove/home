use std::sync::Arc;

use autotrait::autotrait;
use config_types::{TenantConfig, WebConfig};
use credentials::{
    DiscordChannelId, DiscordGuildId, DiscordGuildIdRef, DiscordMessageId, DiscordProfile,
    DiscordRoleId, DiscordRoleIdRef, DiscordUserId, DiscordUserIdRef, UserId,
};
use eyre::{Context, Result};
use facet::Facet;
use futures_core::future::BoxFuture;
use libhttpclient::{
    HttpClient, Uri,
    header::{HeaderName, HeaderValue},
};
use time::OffsetDateTime;
use url::Url;

struct ModImpl {
    client: Arc<dyn HttpClient>,
}

pub fn load() -> &'static dyn Mod {
    use std::sync::OnceLock;

    static MOD: OnceLock<ModImpl> = OnceLock::new();
    MOD.get_or_init(|| ModImpl {
        client: Arc::from(libhttpclient::load().client()),
    })
}

// Note: coolbearbot needs the following permissions:
// Manage Roles, View Channels, View Server Insights, View Server Subscription Insights,
// Send Messages, Embed Links, Attach Files, Create Polls

#[autotrait]
impl Mod for ModImpl {
    fn make_login_url(&self, tc: &TenantConfig, web: WebConfig) -> eyre::Result<String> {
        let discord_secrets = tc.discord_secrets()?;

        let mut u = Url::parse("https://discord.com/api/v10/oauth2/authorize")?;
        {
            let mut q = u.query_pairs_mut();
            q.append_pair("response_type", "code");
            q.append_pair("client_id", &discord_secrets.oauth_client_id);
            q.append_pair("redirect_uri", &make_discord_callback_url(tc, web));
            q.append_pair("scope", "identify");
        }
        Ok(u.to_string())
    }

    fn handle_oauth_callback<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        web: WebConfig,
        args: &'fut DiscordCallbackArgs,
    ) -> BoxFuture<'fut, Result<Option<DiscordCredentials>>> {
        Box::pin(async move {
            let code = match url::form_urlencoded::parse(args.raw_query.as_bytes())
                .find(|(key, _)| key == "code")
                .map(|(_, value)| value.into_owned())
            {
                // that means the user cancelled the oauth flow
                None => return Ok(None),
                Some(code) => code,
            };

            let discord_secrets = tc.discord_secrets()?;

            let res = self
                .client
                .post(Uri::from_static("https://discord.com/api/v10/oauth2/token"))
                .query(&[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", &make_discord_callback_url(tc, web)),
                    ("client_id", &discord_secrets.oauth_client_id),
                    ("client_secret", &discord_secrets.oauth_client_secret),
                ])
                .send()
                .await
                .wrap_err("While getting Discord access token")?;

            if !res.status().is_success() {
                let status = res.status();
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                return Err(eyre::eyre!("got HTTP {status}, server said: {error}"));
            }

            let text = res.text().await?;
            let creds = match facet_json::from_str::<DiscordCredentialsAPI>(&text) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("Got Discord auth error: {text}");
                    return Err(eyre::eyre!("Got Discord auth error: {e}"));
                }
            };

            log::info!(
                "Successfully obtained Discord token with scope {}",
                &creds.scope
            );

            let creds = DiscordCredentials {
                access_token: creds.access_token,
                refresh_token: creds.refresh_token,
                expires_at: OffsetDateTime::now_utc()
                    + time::Duration::seconds(creds.expires_in as i64),
            };

            Ok(Some(creds))
        })
    }

    fn fetch_profile<'fut>(
        &'fut self,
        creds: &'fut DiscordCredentials,
    ) -> BoxFuture<'fut, Result<DiscordProfile>> {
        Box::pin(async move {
            #[derive(Facet)]
            struct DiscordUser {
                id: String,
                username: String,
                global_name: Option<String>,
                avatar: Option<String>,
            }

            let res = self
                .client
                .get(Uri::from_static("https://discord.com/api/v10/users/@me"))
                .polite_user_agent()
                .bearer_auth(&creds.access_token)
                .send()
                .await?;

            if !res.status().is_success() {
                let status = res.status();
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                return Err(eyre::eyre!("got HTTP {status}, server said: {error}"));
            }

            let user = res
                .json::<DiscordUser>()
                .await
                .map_err(|e| eyre::eyre!("{}", e.to_string()))?;

            let profile = DiscordProfile {
                id: DiscordUserId::new(user.id),
                username: user.username,
                global_name: user.global_name,
                avatar_hash: user.avatar,
            };

            log::info!("Discord profile: {profile:#?}");
            Ok(profile)
        })
    }

    fn refresh_credentials<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        credentials: &'fut DiscordCredentials,
    ) -> BoxFuture<'fut, Result<DiscordCredentials>> {
        Box::pin(async move {
            let discord_secrets = tc.discord_secrets()?;

            let res = self
                .client
                .post(Uri::from_static("https://discord.com/api/v10/oauth2/token"))
                .query(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &credentials.refresh_token),
                    ("client_id", &discord_secrets.oauth_client_id),
                    ("client_secret", &discord_secrets.oauth_client_secret),
                ])
                .send()
                .await
                .map_err(|e| eyre::eyre!("While refreshing Discord access token: {e}"))?;

            if !res.status().is_success() {
                let status = res.status();
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                return Err(eyre::eyre!("got HTTP {status}, server said: {error}"));
            }

            let creds = res.json::<DiscordCredentialsAPI>().await?;
            log::info!(
                "Successfully refreshed Discord token with scope {}",
                &creds.scope
            );

            let creds = DiscordCredentials {
                access_token: creds.access_token,
                refresh_token: creds.refresh_token,
                expires_at: OffsetDateTime::now_utc()
                    + time::Duration::seconds(creds.expires_in as i64),
            };

            Ok(creds)
        })
    }

    //////////////////////////////////////////////////////////////////////
    // Bot endpoints
    //////////////////////////////////////////////////////////////////////

    fn list_bot_guilds<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
    ) -> BoxFuture<'fut, Result<Vec<DiscordGuild>>> {
        Box::pin(async move {
            let uri = v10_uri(
                "/users/@me/guilds",
                &[
                    ("limit", "200"), // Discord's max limit per request
                    ("with_counts", "true"),
                ],
            )?;
            let guilds = json_req::<Vec<DiscordGuild>>(tc, self.client.get(uri)).await?;
            log::info!("Successfully fetched {} bot guilds", guilds.len());
            Ok(guilds)
        })
    }

    fn list_guild_members<'fut>(
        &'fut self,
        guild_id: &'fut DiscordGuildIdRef,
        tc: &'fut TenantConfig,
    ) -> BoxFuture<'fut, Result<Vec<DiscordGuildMember>>> {
        Box::pin(async move {
            let uri = v10_uri(
                &format!("/guilds/{guild_id}/members"),
                &[("limit", "1000")], // Discord's max limit per request
            )?;
            let members = json_req::<Vec<DiscordGuildMember>>(tc, self.client.get(uri)).await?;

            log::info!(
                "Successfully fetched {} guild members for guild {}",
                members.len(),
                guild_id
            );
            Ok(members)
        })
    }

    fn list_guild_roles<'fut>(
        &'fut self,
        guild_id: &'fut DiscordGuildIdRef,
        tc: &'fut TenantConfig,
    ) -> BoxFuture<'fut, Result<Vec<DiscordRole>>> {
        Box::pin(async move {
            let uri = v10_uri(&format!("/guilds/{guild_id}/roles"), &[])?;
            let roles = json_req::<Vec<DiscordRole>>(tc, self.client.get(uri)).await?;
            log::info!("Successfully fetched {} guild roles", roles.len());
            Ok(roles)
        })
    }

    fn add_guild_member_role<'fut>(
        &'fut self,
        guild_id: &'fut DiscordGuildIdRef,
        user_id: &'fut DiscordUserIdRef,
        role_id: &'fut DiscordRoleIdRef,
        tc: &'fut TenantConfig,
    ) -> BoxFuture<'fut, Result<()>> {
        Box::pin(async move {
            let uri = v10_uri(
                &format!("/guilds/{guild_id}/members/{user_id}/roles/{role_id}"),
                &[],
            )?;

            let _text = text_req(tc, self.client.put(uri)).await?;

            log::info!("Successfully added role {role_id} to user {user_id} in guild {guild_id}");
            Ok(())
        })
    }

    fn remove_guild_member_role<'fut>(
        &'fut self,
        guild_id: &'fut DiscordGuildIdRef,
        user_id: &'fut DiscordUserIdRef,
        role_id: &'fut DiscordRoleIdRef,
        tc: &'fut TenantConfig,
    ) -> BoxFuture<'fut, Result<()>> {
        Box::pin(async move {
            let uri = v10_uri(
                &format!("/guilds/{guild_id}/members/{user_id}/roles/{role_id}"),
                &[],
            )?;

            let _text = text_req(tc, self.client.delete(uri)).await?;

            log::info!(
                "Successfully removed role {role_id} from user {user_id} in guild {guild_id}"
            );
            Ok(())
        })
    }

    fn list_guild_channels<'fut>(
        &'fut self,
        guild_id: &'fut DiscordGuildIdRef,
        tc: &'fut TenantConfig,
    ) -> BoxFuture<'fut, Result<Vec<DiscordChannel>>> {
        Box::pin(async move {
            let uri = v10_uri(&format!("/guilds/{guild_id}/channels"), &[])?;
            let channels = json_req::<Vec<DiscordChannel>>(tc, self.client.get(uri)).await?;
            log::info!(
                "Successfully fetched {} channels for guild {guild_id}",
                channels.len()
            );
            Ok(channels)
        })
    }

    fn post_message_to_channel<'fut>(
        &'fut self,
        channel_id: &'fut DiscordChannelId,
        content: &'fut str,
        tc: &'fut TenantConfig,
    ) -> BoxFuture<'fut, Result<DiscordMessage>> {
        Box::pin(async move {
            let uri = v10_uri(&format!("/channels/{channel_id}/messages"), &[])?;

            let message_payload = DiscordMessagePayload {
                content: content.to_string(),
            };

            let req = self.client.post(uri).json(&message_payload)?;
            let message = json_req::<DiscordMessage>(tc, req).await?;

            log::info!("Successfully posted message to channel {channel_id}");
            Ok(message)
        })
    }

    fn get_guild_member<'fut>(
        &'fut self,
        guild_id: &'fut DiscordGuildIdRef,
        user_id: &'fut DiscordUserIdRef,
        tc: &'fut TenantConfig,
    ) -> BoxFuture<'fut, Result<DiscordGuildMember>> {
        Box::pin(async move {
            let uri = v10_uri(&format!("/guilds/{guild_id}/members/{user_id}"), &[])?;
            let member = json_req::<DiscordGuildMember>(tc, self.client.get(uri)).await?;
            log::info!("Successfully fetched guild member {user_id} for guild {guild_id}");
            Ok(member)
        })
    }
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordCallbackArgs {
    pub raw_query: String,

    /// if we're linking this discord account to an existing UserID, this is set
    #[facet(default)]
    pub logged_in_user_id: Option<UserId>,
}

#[derive(Debug, Clone, Facet)]
struct DiscordCredentialsAPI {
    /// example: "6qrZcUqja7812RVdnEKjpzOL4CvHBFG"
    access_token: String,
    /// example: "D43f5y0ahjqew82jZ4NViEr2YafMKhue"
    refresh_token: String,
    /// example: "bearer"
    token_type: String,
    /// example: "identify"
    scope: String,
    /// Seconds until expiration, typically 604800 (7 days)
    expires_in: u32,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordCredentials {
    /// example: "6qrZcUqja7812RVdnEKjpzOL4CvHBFG"
    pub access_token: String,
    /// example: "D43f5y0ahjqew82jZ4NViEr2YafMKhue"
    pub refresh_token: String,
    /// When the token expires
    pub expires_at: OffsetDateTime,
}

pub(crate) fn make_discord_callback_url(tc: &TenantConfig, web: WebConfig) -> String {
    let base_url = tc.web_base_url(web);
    let url = format!("{base_url}/login/discord/callback");
    log::info!("Crafted discord callback url: {url}");
    url
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordUnlinkArgs {
    pub logged_in_user_id: UserId,
}

impl DiscordCredentials {
    pub fn expire_soon(&self) -> bool {
        let now = OffsetDateTime::now_utc();
        let one_hour = time::Duration::hours(1);
        self.expires_at - now < one_hour
    }
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordUser {
    pub id: DiscordUserId,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordGuildMember {
    /// The user this guild member represents
    pub user: Option<DiscordUser>,
    /// This user's guild nickname
    pub nick: Option<String>,
    /// Array of role object ids
    pub roles: Vec<DiscordRoleId>,
    /// When the user joined the guild
    pub joined_at: Option<String>,
    /// When the user started boosting the guild
    pub premium_since: Option<String>,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordRole {
    /// Role id
    pub id: DiscordRoleId,
    /// Role name
    pub name: String,
    /// Integer representation of hexadecimal color code
    pub color: u32,
    /// If this role is pinned in the user listing
    pub hoist: bool,
    /// Role icon hash
    pub icon: Option<String>,
    /// Role unicode emoji
    pub unicode_emoji: Option<String>,
    /// Position of this role
    pub position: i32,
    /// Permission bit set
    pub permissions: String,
    /// Whether this role is managed by an integration
    pub managed: bool,
    /// Whether this role is mentionable
    pub mentionable: bool,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordGuild {
    /// Guild id
    pub id: DiscordGuildId,
    /// Guild name (2-100 characters, excluding trailing and leading whitespace)
    pub name: String,
    /// Icon hash
    pub icon: Option<String>,
    /// True if the user is the owner of the guild
    pub owner: Option<bool>,
    /// Total permissions for the user in the guild (excludes overwrites)
    pub permissions: Option<String>,
    /// Enabled guild features
    pub features: Vec<String>,
    /// Approximate number of members in this guild
    #[facet(default)]
    pub approximate_member_count: Option<u32>,
    /// Approximate number of non-offline members in this guild
    #[facet(default)]
    pub approximate_presence_count: Option<u32>,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordChannel {
    /// Channel id
    pub id: DiscordChannelId,
    /// Channel name
    pub name: String,
    /// Channel type (0 = text, 2 = voice, etc.)
    pub r#type: u8,
    /// Channel topic (for text channels)
    #[facet(default)]
    pub topic: Option<String>,
    /// Channel position
    #[facet(default)]
    pub position: Option<i32>,
    /// Channel permission overwrites
    #[facet(default)]
    pub permission_overwrites: Vec<DiscordPermissionOverwrite>,
    /// Channel parent id (for threads/categories)
    #[facet(default)]
    pub parent_id: Option<DiscordChannelId>,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordPermissionOverwrite {
    /// Role or user id
    pub id: String,
    /// Type of overwrite (0 = role, 1 = member)
    pub r#type: u8,
    /// Permission bit set for allowed permissions
    pub allow: String,
    /// Permission bit set for denied permissions
    pub deny: String,
}

#[derive(Debug, Clone, Facet)]
struct DiscordMessagePayload {
    content: String,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordMessage {
    /// Message id
    pub id: DiscordMessageId,
    /// Channel id this message was sent in
    pub channel_id: DiscordChannelId,
    /// Author of this message
    pub author: DiscordUser,
    /// Contents of the message
    pub content: String,
    /// When this message was sent
    pub timestamp: String,
    /// When this message was edited (if it was edited)
    pub edited_timestamp: Option<String>,
    /// Whether this was a TTS message
    pub tts: bool,
    /// Whether this message mentions everyone
    pub mention_everyone: bool,
}

fn v10_uri(path: &str, query_params: &[(&str, &str)]) -> eyre::Result<Uri> {
    if !path.starts_with('/') {
        panic!("someone forgot the leading slash in libdiscord");
    }
    let mut url = Url::parse(&format!("https://discord.com/api/v10{path}"))?;

    if !query_params.is_empty() {
        let mut q = url.query_pairs_mut();
        for (key, value) in query_params {
            q.append_pair(key, value);
        }
    }

    url.to_string()
        .parse()
        .map_err(|e| eyre::eyre!("Invalid URL: {}", e))
}

async fn text_req(
    tc: &TenantConfig,
    req: Box<dyn libhttpclient::RequestBuilder>,
) -> eyre::Result<String> {
    let discord_secrets = tc.discord_secrets()?;

    let res = req
        .polite_user_agent()
        .header(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        )
        .header(
            HeaderName::from_static("authorization"),
            HeaderValue::from_str(&format!("Bot {}", discord_secrets.bot_token))
                .map_err(|e| eyre::eyre!("Invalid bot token: {}", e))?,
        )
        .send()
        .await
        .wrap_err("While sending request")?;

    if !res.status().is_success() {
        let status = res.status();
        let error = res
            .text()
            .await
            .unwrap_or_else(|_| "Could not get error text".into());
        return Err(eyre::eyre!("got HTTP {status}, server said: {error}"));
    }

    let text = res.text().await?;
    Ok(text)
}

async fn json_req<T: for<'de> Facet<'de>>(
    tc: &TenantConfig,
    req: Box<dyn libhttpclient::RequestBuilder>,
) -> eyre::Result<T> {
    let text = text_req(tc, req).await?;
    match facet_json::from_str::<T>(&text) {
        Ok(result) => Ok(result),
        Err(e) => {
            log::warn!("Failed to parse response: {e}");
            log::warn!("Full response text: {text}");
            Err(eyre::eyre!("Failed to parse response: {e}"))
        }
    }
}
