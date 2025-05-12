use std::collections::HashSet;

use config_types::{RevisionConfig, TenantConfig, WebConfig};
use credentials::{AuthBundle, Profile, Tier, UserInfo};
use eyre::Result;
use facet::Facet;
use libhttpclient::{HeaderValue, HttpClient, Uri, header};
use time::OffsetDateTime;
use tracing::debug;

use crate::{GitHubCallbackArgs, GitHubCredentials, ModImpl};

impl ModImpl {
    pub(crate) async fn handle_oauth_callback_unboxed(
        &self,
        tc: &TenantConfig,
        web: WebConfig,
        args: &GitHubCallbackArgs,
    ) -> eyre::Result<Option<GitHubCredentials>> {
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

        let creds = res.json::<GitHubCredentials>().await?;
        tracing::info!(
            "Successfully obtained GitHub token with scope {}",
            &creds.scope
        );

        Ok(Some(creds))
    }

    pub(crate) async fn list_sponsors_unboxed(
        &self,
        _tc: &TenantConfig,
        client: &dyn HttpClient,
        github_creds: &GitHubCredentials,
    ) -> eyre::Result<HashSet<String>> {
        let mut credited_patrons: HashSet<String> = Default::default();
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
                        tracing::error!("GitHub API error: {:?}", error);
                    }
                }
                // still return the sponsors we got so far
                return Ok(credited_patrons);
            }

            let data = match res.data {
                Some(data) => data,
                None => {
                    let err = eyre::eyre!("got no data from GitHub API");
                    tracing::error!("{err}");
                    // still return the sponsors we got so far
                    return Ok(credited_patrons);
                }
            };

            let viewer = &data.viewer;

            for sponsor in &viewer.sponsors.nodes {
                if let Some(sponsorship) = sponsor.sponsorshipForViewerAsSponsorable.as_ref() {
                    if sponsorship.privacyLevel != "PUBLIC" {
                        continue;
                    }

                    if sponsorship.tier.isOneTime {
                        continue;
                    }

                    if let Some(price) = sponsorship.tier.monthlyPriceInDollars {
                        if price < 10 {
                            continue;
                        }
                    }

                    let name = sponsor.name.as_ref().unwrap_or(&sponsor.login);
                    credited_patrons.insert(name.trim().to_string());
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

        Ok(credited_patrons)
    }

    pub async fn to_site_credentials_unboxed(
        &self,
        rc: &RevisionConfig,
        web: WebConfig,
        github_creds: &GitHubCredentials,
    ) -> Result<(GitHubCredentials, AuthBundle)> {
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
            tier: SponsorshipTier,
        }

        #[derive(Facet)]
        #[allow(non_snake_case)]
        struct SponsorshipTier {
            isOneTime: bool,
            monthlyPriceInDollars: u32,
        }

        let query = include_str!("github_sponsorship_for_viewer.graphql");
        let login = if web.env.is_dev() {
            // just testing!
            "gennyble"
        } else {
            "fasterthanlime"
        };
        let variables = Variables { login };

        let res = libhttpclient::load()
            .client()
            .post(Uri::from_static("https://api.github.com/graphql"))
            .polite_user_agent()
            .json(&GraphqlQuery {
                query: query.into(),
                variables,
            })?
            .bearer_auth(&github_creds.access_token)
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
        let full_name = viewer.name.as_ref().unwrap_or(&viewer.login);
        let viewer_github_user_id = viewer.databaseId.to_string();

        let tier = response
            .data
            .user
            .sponsorshipForViewerAsSponsor
            .and_then(|s| {
                if s.tier.isOneTime {
                    return None;
                }

                match s.tier.monthlyPriceInDollars {
                    _ if web.env.is_dev() => Some(Tier {
                        title: "Silver".into(),
                    }),
                    0..10 => Some(Tier {
                        title: "Bronze".into(),
                    }),
                    10..50 => Some(Tier {
                        title: "Silver".into(),
                    }),
                    50.. => Some(Tier {
                        title: "Gold".into(),
                    }),
                }
            })
            .or_else(|| {
                eprintln!("admin github ids: {:?}", rc.admin_github_ids);
                eprintln!("viewer github user id: {viewer_github_user_id}");

                if rc
                    .admin_github_ids
                    .iter()
                    .any(|id| id == viewer_github_user_id.as_str())
                {
                    creator_tier_name().map(|title| Tier { title })
                } else {
                    None
                }
            });

        tracing::info!(
            "GitHub user \x1b[33m{:?}\x1b[0m (ID: \x1b[36m{:?}\x1b[0m, name: \x1b[32m{:?}\x1b[0m, tier: \x1b[35m{:?}\x1b[0m) logged in",
            viewer.login,
            viewer.databaseId,
            viewer.name,
            tier
        );

        let auth_bundle = AuthBundle {
            expires_at: OffsetDateTime::now_utc() + time::Duration::days(365),
            user_info: UserInfo {
                profile: Profile {
                    full_name: full_name.to_owned(),
                    patreon_id: None,
                    github_id: Some(viewer_github_user_id.clone()),
                    thumb_url: viewer.avatarUrl.to_owned(),
                },
                tier,
            },
        };

        Ok((github_creds.clone(), auth_bundle))
    }
}

fn creator_tier_name() -> Option<String> {
    let path = std::path::Path::new("/tmp/home-creator-tier-override");
    match fs_err::read_to_string(path) {
        Ok(contents) => {
            let name = contents.trim().to_string();
            eprintln!("🎭 Pretending creator has tier name {name}");
            Some(name)
        }
        Err(_) => {
            eprintln!(
                "🔒 Creator special casing \x1b[31mdisabled\x1b[0m - create /tmp/home-creator-tier-override with tier name like 'Bronze' or 'Silver' to enable 🔑"
            );
            None
        }
    }
}

pub(crate) fn make_github_callback_url(tc: &TenantConfig, web: WebConfig) -> String {
    let base_url = tc.web_base_url(web);
    format!("{base_url}/login/github/callback")
}
