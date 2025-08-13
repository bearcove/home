use autotrait::autotrait;
use config_types::{TenantConfig, WebConfig};
use credentials::{DiscordProfile, DiscordUserId, UserId};
use eyre::{Context, Result};
use facet::Facet;
use futures_core::future::BoxFuture;
use libhttpclient::{HttpClient, Uri};
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

pub(crate) fn make_discord_callback_url(tc: &TenantConfig, web: WebConfig) -> String {
    let base_url = tc.web_base_url(web);
    let url = format!("{base_url}/login/discord/callback");
    log::info!("Crafted discord callback url: {url}");
    url
}
