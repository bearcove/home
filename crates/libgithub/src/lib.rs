#![allow(non_snake_case)]

use autotrait::autotrait;
use credentials::{GithubProfile, GithubUserId};
use facet::Facet;
use futures_core::future::BoxFuture;
use libhttpclient::{HeaderValue, HttpClient, Uri, header};

use config_types::{TenantConfig, WebConfig};
use eyre::Result;
use log::debug;
use time::OffsetDateTime;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn make_login_url(
        &self,
        tc: &TenantConfig,
        web: WebConfig,
        kind: GithubLoginPurpose,
    ) -> eyre::Result<String> {
        use url::Url;
        let github_secrets = &tc.github_secrets()?;

        let mut u = Url::parse("https://github.com/login/oauth/authorize")?;
        {
            let mut q = u.query_pairs_mut();
            q.append_pair("response_type", "code");
            q.append_pair("client_id", &github_secrets.oauth_client_id);
            q.append_pair("redirect_uri", &make_github_callback_url(tc, web));
            q.append_pair("scope", github_login_purpose_to_scopes(&kind));
        }
        Ok(u.to_string())
    }

    fn handle_oauth_callback<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        web: WebConfig,
        args: &'fut GithubCallbackArgs,
    ) -> BoxFuture<'fut, Result<Option<GithubCredentials>>> {
        Box::pin(async move {
            let code = match url::form_urlencoded::parse(args.raw_query.as_bytes())
                .find(|(key, _)| key == "code")
                .map(|(_, value)| value.into_owned())
            {
                // that means the user cancelled the oauth flow
                None => return Ok(None),
                Some(code) => code,
            };

            let gh_sec = tc.github_secrets()?;

            let res = libhttpclient::load()
                .client()
                .post(Uri::from_static(
                    "https://github.com/login/oauth/access_token",
                ))
                .query(&[
                    ("client_id", &gh_sec.oauth_client_id),
                    ("client_secret", &gh_sec.oauth_client_secret),
                    ("redirect_uri", &make_github_callback_url(tc, web)),
                    ("code", code.as_ref()),
                ])
                .header(header::ACCEPT, HeaderValue::from_static("application/json"))
                .send_and_expect_200()
                .await
                .map_err(|e| eyre::eyre!("While getting GitHub access token: {e}"))?;

            let text = res.text().await?;
            let creds = match facet_json::from_str::<GithubCredentialsAPI>(&text) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("Got GitHub auth error: {text}");
                    return Err(eyre::eyre!("Got Github auth error: {e}"));
                }
            };
            log::info!(
                "Successfully obtained GitHub token with scope {}",
                &creds.scope
            );

            let creds = GithubCredentials {
                access_token: creds.access_token,
                scope: creds.scope,
                expires_at: OffsetDateTime::now_utc() + default_expires_in(),
            };

            Ok(Some(creds))
        })
    }

    fn fetch_profile<'fut>(
        &'fut self,
        creds: &'fut GithubCredentials,
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<GithubProfile>> {
        Box::pin(async move {
            #[derive(Facet)]
            struct GraphqlQuery {
                query: String,
                variables: Variables,
            }

            #[derive(Facet)]
            struct Variables {
                login: &'static str,
            }

            #[derive(Facet)]
            struct GraphqlResponse {
                data: GraphqlResponseData,
            }

            #[derive(Facet)]
            struct GraphqlResponseData {
                viewer: Viewer,
                user: User,
            }
            #[derive(Facet)]
            #[allow(non_snake_case)]
            struct Viewer {
                databaseId: i64,
                login: String,
                name: Option<String>,
                avatarUrl: String,
            }

            #[derive(Facet)]
            #[allow(non_snake_case)]
            struct User {
                sponsorshipForViewerAsSponsor: Option<Sponsorship>,
            }

            #[derive(Facet)]
            struct Sponsorship {
                privacyLevel: String,
                tier: SponsorshipTier,
            }

            #[derive(Facet)]
            #[allow(non_snake_case)]
            struct SponsorshipTier {
                isOneTime: bool,
                monthlyPriceInDollars: u32,
            }

            let query = include_str!("github_sponsorship_for_viewer.graphql");
            // well this should be using databaseId I think, for the first GithubUserId in the tenant config
            let login = "fasterthanlime";
            let variables = Variables { login };

            let res = client
                .post(Uri::from_static("https://api.github.com/graphql"))
                .polite_user_agent()
                .json(&GraphqlQuery {
                    query: query.into(),
                    variables,
                })?
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

            let response = res
                .json::<GraphqlResponse>()
                .await
                .map_err(|e| eyre::eyre!("{}", e.to_string()))?;

            let viewer = &response.data.viewer;
            let profile = GithubProfile {
                id: GithubUserId::new(viewer.databaseId.to_string()),
                monthly_usd: response
                    .data
                    .user
                    .sponsorshipForViewerAsSponsor
                    .as_ref()
                    .and_then(|s| {
                        if s.tier.isOneTime {
                            None
                        } else {
                            Some(s.tier.monthlyPriceInDollars as u64)
                        }
                    }),
                sponsorship_privacy_level: response
                    .data
                    .user
                    .sponsorshipForViewerAsSponsor
                    .as_ref()
                    .map(|s| s.privacyLevel.clone()),
                name: viewer.name.clone(),
                login: viewer.login.clone(),
                avatar_url: Some(viewer.avatarUrl.clone()),
            };

            log::info!("GitHub profile: {profile:#?}");
            Ok(profile)
        })
    }

    fn refresh_credentials<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        credentials: &'fut GithubCredentials,
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<GithubCredentials>> {
        Box::pin(async move {
            let gh_sec = tc.github_secrets()?;

            let res = client
                .post(Uri::from_static(
                    "https://github.com/login/oauth/access_token",
                ))
                .query(&[
                    ("client_id", &gh_sec.oauth_client_id),
                    ("client_secret", &gh_sec.oauth_client_secret),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &credentials.access_token),
                ])
                .header(header::ACCEPT, HeaderValue::from_static("application/json"))
                .send_and_expect_200()
                .await
                .map_err(|e| eyre::eyre!("While refreshing GitHub access token: {e}"))?;

            let creds = res.json::<GithubCredentialsAPI>().await?;
            log::info!(
                "Successfully refreshed GitHub token with scope {}",
                &creds.scope
            );

            let creds = GithubCredentials {
                access_token: creds.access_token,
                scope: creds.scope,
                expires_at: OffsetDateTime::now_utc() + default_expires_in(),
            };

            Ok(creds)
        })
    }

    fn list_sponsors<'fut>(
        &'fut self,
        client: &'fut dyn HttpClient,
        github_creds: &'fut GithubCredentials,
    ) -> BoxFuture<'fut, Result<Vec<GithubProfile>>> {
        Box::pin(async move {
            let mut github_profiles: Vec<GithubProfile> = Vec::new();
            let query = include_str!("github_sponsors.graphql");

            #[derive(Facet)]
            struct GraphqlQuery {
                query: String,
                variables: Variables,
            }

            #[derive(Facet)]
            struct GraphqlResponse {
                data: Option<GraphqlResponseData>,
                errors: Option<Vec<GraphqlError>>,
            }

            #[derive(Facet, Debug)]
            struct GraphqlError {
                #[allow(dead_code)]
                message: String,
            }

            #[derive(Facet)]
            struct GraphqlResponseData {
                viewer: Viewer,
            }

            #[derive(Facet)]
            struct Viewer {
                sponsors: Sponsors,
            }

            #[derive(Facet)]
            #[allow(non_snake_case)]
            struct Sponsors {
                pageInfo: PageInfo,
                nodes: Vec<Node>,
            }

            #[derive(Facet)]
            #[allow(non_snake_case)]
            struct PageInfo {
                endCursor: Option<String>,
            }

            #[derive(Facet)]
            #[allow(non_snake_case)]
            struct Node {
                login: String,
                name: Option<String>,
                avatarUrl: Option<String>,
                sponsorshipForViewerAsSponsorable: Option<SponsorshipForViewerAsSponsorable>,
            }

            #[derive(Facet)]
            #[allow(non_snake_case)]
            struct SponsorshipForViewerAsSponsorable {
                privacyLevel: String,
                tier: GitHubTier,
            }

            #[derive(Facet)]
            #[allow(non_snake_case)]
            struct GitHubTier {
                monthlyPriceInDollars: Option<u32>,
                isOneTime: bool,
            }

            #[derive(Debug, Facet)]
            struct Variables {
                first: u32,
                after: Option<String>,
            }

            let mut query = GraphqlQuery {
                query: query.into(),
                variables: Variables {
                    first: 100,
                    after: None,
                },
            };

            let mut page_num = 0;
            loop {
                page_num += 1;
                debug!("Fetching GitHub page {page_num}");

                let res = client
                    .post(Uri::from_static("https://api.github.com/graphql"))
                    .polite_user_agent()
                    .json(&query)?
                    .bearer_auth(&github_creds.access_token)
                    .send()
                    .await?;

                if !res.status().is_success() {
                    let status = res.status();
                    let error = res
                        .text()
                        .await
                        .unwrap_or_else(|_| "Could not get error text".into());
                    let err = eyre::eyre!(format!("got HTTP {status}, server said: {error}"));
                    return Err(err);
                }

                let res = res
                    .json::<GraphqlResponse>()
                    .await
                    .map_err(|e| eyre::eyre!("could not deserialize GitHub API response: {e}"))?;

                if let Some(errors) = res.errors {
                    fn is_error_ignored(error: &GraphqlError) -> bool {
                        // Sample error message: Although you appear to have the correct
                        // authorization credentials, the `xelforce` organization has
                        // enabled OAuth App access restrictions, meaning that data
                        // access to third-parties is limited. For more information on
                        // these restrictions, including how to enable this app, visit
                        // https://docs.github.com/articles/restricting-access-to-your-organization-s-data/
                        //
                        // In this case GitHub still gives us access to the rest of the
                        // data so we don't actually need to do anything about this
                        // error except for ignoring it
                        error.message.contains("OAuth App access restrictions")
                    }

                    for error in errors {
                        if !is_error_ignored(&error) {
                            log::error!("GitHub API error: {error:?}");
                        }
                    }
                    // still return the sponsors we got so far
                    return Ok(github_profiles);
                }

                let data = match res.data {
                    Some(data) => data,
                    None => {
                        let err = eyre::eyre!("got no data from GitHub API");
                        log::error!("{err}");
                        // still return the sponsors we got so far
                        return Ok(github_profiles);
                    }
                };

                let viewer = &data.viewer;

                for sponsor in &viewer.sponsors.nodes {
                    if let Some(sponsorship) = sponsor.sponsorshipForViewerAsSponsorable.as_ref() {
                        let monthly_usd = if sponsorship.tier.isOneTime {
                            None
                        } else {
                            sponsorship.tier.monthlyPriceInDollars.map(|p| p as u64)
                        };

                        github_profiles.push(GithubProfile {
                            id: GithubUserId::new(sponsor.login.clone()),
                            monthly_usd,
                            sponsorship_privacy_level: Some(sponsorship.privacyLevel.clone()),
                            name: sponsor.name.clone(),
                            login: sponsor.login.clone(),
                            avatar_url: sponsor.avatarUrl.clone(),
                        });
                    }
                }

                match viewer.sponsors.pageInfo.endCursor.as_ref() {
                    Some(end_cursor) => {
                        query.variables.after = Some(end_cursor.clone());
                    }
                    None => {
                        // all done!
                        break;
                    }
                }
            }

            Ok(github_profiles)
        })
    }
}

#[derive(Debug, Clone, Facet)]
pub struct GithubCallbackArgs {
    pub raw_query: String,
}

#[derive(Debug, Clone, Facet)]
struct GithubCredentialsAPI {
    /// example: "ajba90sd098w0e98f0w9e8g90a8ed098wgfae_w"
    access_token: String,
    /// example: "read:user"
    scope: String,
    /// example: "bearer"
    token_type: Option<String>,
}

/*
note: github errors look like: {"error":"bad_verification_code","error_description":"The code passed is incorrect or expired.","error_uri":"https://docs.github.com/apps/managing-oauth-apps/troubleshooting-oauth-app-access-token-request-errors/#bad-verification-code"}
*/

#[derive(Debug, Clone, Facet)]
pub struct GithubCredentials {
    /// example: "ajba90sd098w0e98f0w9e8g90a8ed098wgfae_w"
    pub access_token: String,
    /// example: "read:user"
    pub scope: String,
    /// usually 8 hours for github, see https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/refreshing-user-access-tokens
    pub expires_at: OffsetDateTime,
}

impl GithubCredentials {
    pub fn expire_soon(&self) -> bool {
        let now = OffsetDateTime::now_utc();
        let twenty_four_hours = time::Duration::hours(1);
        self.expires_at - now < twenty_four_hours
    }
}

/// The purpose of the login (to determine the OAuth scopes needed for the login)
pub enum GithubLoginPurpose {
    // admin login
    Admin,
    // normal user login
    Regular,
}

/// Returns GitHub OAuth scopes needed for the login
pub fn github_login_purpose_to_scopes(purpose: &GithubLoginPurpose) -> &'static str {
    match purpose {
        GithubLoginPurpose::Admin => "read:user,read:org",
        GithubLoginPurpose::Regular => "read:user",
    }
}

pub(crate) fn make_github_callback_url(tc: &TenantConfig, web: WebConfig) -> String {
    let base_url = tc.web_base_url(web);
    let url = format!("{base_url}/login/github/callback");
    log::info!("Crafted github callback url: {url}");
    url
}

// GitHub doesn't set expires_in for OAuth apps, they say it expires after it hasn't
// been used in a year, but let's be more conservative:
fn default_expires_in() -> time::Duration {
    time::Duration::seconds(31 * 24 * 60 * 60) // 31 days
}
