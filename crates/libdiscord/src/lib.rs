use autotrait::autotrait;
use config_types::{TenantConfig, WebConfig};
use credentials::{DiscordProfile, DiscordUserId, UserId};
use eyre::{Context, Result};
use facet::Facet;
use futures_core::future::BoxFuture;
use libhttpclient::{
    HttpClient, Uri,
    header::{HeaderName, HeaderValue},
};
use time::OffsetDateTime;
use url::Url;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

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

            let res = libhttpclient::load()
                .client()
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
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<DiscordProfile>> {
        Box::pin(async move {
            #[derive(Facet)]
            struct DiscordUser {
                id: String,
                username: String,
                global_name: Option<String>,
                avatar: Option<String>,
            }

            let res = client
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
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<DiscordCredentials>> {
        Box::pin(async move {
            let discord_secrets = tc.discord_secrets()?;

            let res = client
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

    fn list_bot_guilds<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<Vec<DiscordGuild>>> {
        Box::pin(async move {
            let discord_secrets = tc.discord_secrets()?;
            let mut u = Url::parse("https://discord.com/api/v10/users/@me/guilds")?;
            {
                let mut q = u.query_pairs_mut();
                q.append_pair("limit", "200"); // Discord's max limit per request
            }
            let uri: Uri = u
                .to_string()
                .parse()
                .map_err(|e| eyre::eyre!("Invalid URL: {}", e))?;

            let res = client
                .get(uri)
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
                .wrap_err("While fetching bot guilds")?;

            if !res.status().is_success() {
                let status = res.status();
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                return Err(eyre::eyre!("got HTTP {status}, server said: {error}"));
            }

            let guilds = res
                .json::<Vec<DiscordGuildAPI>>()
                .await
                .map_err(|e| eyre::eyre!("Failed to parse guilds response: {}", e))?;

            let bot_guilds: Vec<DiscordGuild> = guilds
                .into_iter()
                .map(|g| DiscordGuild {
                    id: g.id,
                    name: g.name,
                    icon: g.icon,
                    owner: g.owner,
                    permissions: g.permissions,
                    features: g.features,
                })
                .collect();

            log::info!("Successfully fetched {} bot guilds", bot_guilds.len());
            Ok(bot_guilds)
        })
    }

    fn list_guild_members<'fut>(
        &'fut self,
        guild_id: &'fut str,
        tc: &'fut TenantConfig,
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<Vec<DiscordGuildMember>>> {
        Box::pin(async move {
            let discord_secrets = tc.discord_secrets()?;
            let url = format!("https://discord.com/api/v10/guilds/{guild_id}/members");
            let uri: Uri = url.parse().map_err(|e| eyre::eyre!("Invalid URL: {}", e))?;

            let res = client
                .get(uri)
                .polite_user_agent()
                .header(
                    HeaderName::from_static("authorization"),
                    HeaderValue::from_str(&format!("Bot {}", discord_secrets.bot_token))
                        .map_err(|e| eyre::eyre!("Invalid bot token: {}", e))?,
                )
                .query(&[("limit", "1000")]) // Discord's max limit per request
                .send()
                .await
                .wrap_err("While fetching guild members")?;

            if !res.status().is_success() {
                let status = res.status();
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                return Err(eyre::eyre!("got HTTP {status}, server said: {error}"));
            }

            let members = res
                .json::<Vec<DiscordGuildMemberAPI>>()
                .await
                .map_err(|e| eyre::eyre!("Failed to parse guild members response: {}", e))?;

            let guild_members: Vec<DiscordGuildMember> = members
                .into_iter()
                .map(|m| DiscordGuildMember {
                    user: m.user.map(|u| DiscordUser {
                        id: DiscordUserId::new(u.id),
                        username: u.username,
                        global_name: u.global_name,
                        avatar: u.avatar,
                    }),
                    nick: m.nick,
                    roles: m.roles,
                    joined_at: m.joined_at,
                    premium_since: m.premium_since,
                })
                .collect();

            log::info!("Successfully fetched {} guild members", guild_members.len());
            Ok(guild_members)
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
struct DiscordUserAPI {
    id: String,
    username: String,
    global_name: Option<String>,
    avatar: Option<String>,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordGuildMember {
    /// The user this guild member represents
    pub user: Option<DiscordUser>,
    /// This user's guild nickname
    pub nick: Option<String>,
    /// Array of role object ids
    pub roles: Vec<String>,
    /// When the user joined the guild
    pub joined_at: Option<String>,
    /// When the user started boosting the guild
    pub premium_since: Option<String>,
}

#[derive(Debug, Clone, Facet)]
struct DiscordGuildMemberAPI {
    /// The user this guild member represents
    user: Option<DiscordUserAPI>,
    /// This user's guild nickname
    nick: Option<String>,
    /// Array of role object ids
    roles: Vec<String>,
    /// When the user joined the guild
    joined_at: Option<String>,
    /// When the user started boosting the guild
    premium_since: Option<String>,
}

#[derive(Debug, Clone, Facet)]
pub struct DiscordGuild {
    /// Guild id
    pub id: String,
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
}

#[derive(Debug, Clone, Facet)]
struct DiscordGuildAPI {
    /// Guild id
    id: String,
    /// Guild name (2-100 characters, excluding trailing and leading whitespace)
    name: String,
    /// Icon hash
    icon: Option<String>,
    /// True if the user is the owner of the guild
    owner: Option<bool>,
    /// Total permissions for the user in the guild (excludes overwrites)
    permissions: Option<String>,
    /// Enabled guild features
    features: Vec<String>,
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
