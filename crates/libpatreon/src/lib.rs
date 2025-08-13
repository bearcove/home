use autotrait::autotrait;
use config_types::{RevisionConfig, TenantConfig, WebConfig};
use credentials::PatreonProfile;
use credentials::PatreonUserId;
use credentials::UserId;
use eyre::Context as _;
use eyre::Result;
use facet::Facet;
use futures_core::future::BoxFuture;
use libhttpclient::{HttpClient, Uri};
use time::OffsetDateTime;
use url::Url;

use std::collections::HashMap;

mod jsonapi_ext;
use jsonapi_ext::*;

mod model;
use model::*;

pub struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn make_login_url(&self, web: WebConfig, tc: &TenantConfig) -> Result<String> {
        let patreon_secrets = tc.patreon_secrets()?;
        let mut u = Url::parse("https://patreon.com/oauth2/authorize")?;
        let mut q = u.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair("client_id", &patreon_secrets.oauth_client_id);
        q.append_pair("redirect_uri", &self.make_patreon_callback_url(tc, web));
        q.append_pair("scope", "identity identity.memberships");
        drop(q);

        Ok(u.to_string())
    }

    /// Handles oauth callback, returns credentials or None if the flow was cancelled
    fn handle_oauth_callback<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        web: WebConfig,
        args: &'fut PatreonCallbackArgs,
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<Option<PatreonCredentials>>> {
        Box::pin(async move {
            let code = match url::form_urlencoded::parse(args.raw_query.as_bytes())
                .find(|(key, _)| key == "code")
                .map(|(_, value)| value.into_owned())
            {
                // that means the user cancelled the oauth flow
                None => return Ok(None),
                Some(code) => code,
            };

            let patreon_secrets = tc.patreon_secrets()?;
            let tok_params = {
                let mut serializer = url::form_urlencoded::Serializer::new(String::new());
                serializer.append_pair("code", &code);
                serializer.append_pair("grant_type", "authorization_code");
                serializer.append_pair("client_id", &patreon_secrets.oauth_client_id);
                serializer.append_pair("client_secret", &patreon_secrets.oauth_client_secret);
                serializer.append_pair("redirect_uri", &self.make_patreon_callback_url(tc, web));
                serializer.finish()
            };

            let res = client
                .post(Uri::from_static("https://patreon.com/api/oauth2/token"))
                .form(tok_params)
                .send()
                .await
                .wrap_err("POST to /api/oauth2/token for oauth callback")?;

            let status = res.status();
            if !status.is_success() {
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                return Err(eyre::eyre!("got HTTP {status}, server said: {error}"));
            }

            let text = res.text().await?;
            let creds_api = match facet_json::from_str::<PatreonCredentialsAPI>(&text) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("Got Patreon auth error: {text}");
                    return Err(eyre::eyre!("Got Patreon auth error: {e}"));
                }
            };
            log::info!(
                "Successfully obtained Patreon token with scope {}",
                &creds_api.scope
            );

            let creds = PatreonCredentials {
                access_token: creds_api.access_token,
                refresh_token: creds_api.refresh_token,
                expires_at: OffsetDateTime::now_utc()
                    + time::Duration::seconds(creds_api.expires_in as i64),
            };
            Ok(Some(creds))
        })
    }

    /// Refresh the given patreon credentials
    fn refresh_credentials<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        creds: &'fut PatreonCredentials,
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<PatreonCredentials>> {
        Box::pin(async move {
            let tok_params = {
                let patreon_secrets = tc.patreon_secrets()?;

                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("grant_type", "refresh_token")
                    .append_pair("refresh_token", &creds.refresh_token)
                    .append_pair("client_id", &patreon_secrets.oauth_client_id)
                    .append_pair("client_secret", &patreon_secrets.oauth_client_secret)
                    .finish()
            };
            let uri = Uri::from_static("https://www.patreon.com/api/oauth2/token");
            log::info!("Refresh params: {tok_params}, uri: {uri}");
            let res = client
                .post(uri)
                .form(tok_params)
                .send()
                .await
                .wrap_err("POST to /api/oauth2/token for refresh")?;
            let status = res.status();
            if !status.is_success() {
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                return Err(eyre::eyre!("got HTTP {status}, server said: {error}"));
            }

            let pat_creds = res.json::<PatreonCredentials>().await?;
            log::info!("Successfully refreshed! New credentials: {pat_creds:#?}");

            Ok(pat_creds)
        })
    }

    fn fetch_profile<'fut>(
        &'fut self,
        rc: &'fut RevisionConfig,
        creds: &'fut PatreonCredentials,
        client: &'fut dyn HttpClient,
    ) -> BoxFuture<'fut, Result<PatreonProfile>> {
        Box::pin(async move {
            let mut identity_url = Url::parse("https://www.patreon.com/api/oauth2/v2/identity")?;
            {
                let mut q = identity_url.query_pairs_mut();
                let include = [
                    "memberships",
                    "memberships.currently_entitled_tiers",
                    "memberships.campaign",
                ]
                .join(",");
                q.append_pair("include", &include);
                q.append_pair("fields[member]", "patron_status");
                q.append_pair("fields[user]", "full_name,thumb_url");
                q.append_pair("fields[tier]", "title");
            }

            let identity_url = identity_url.to_string();

            let identity_uri = identity_url.parse::<Uri>().unwrap();
            let res = client
                .get(identity_uri.clone())
                .bearer_auth(&creds.access_token)
                .send()
                .await
                .wrap_err("GET /api/oauth2/v2/identity")?;

            let status = res.status();
            if !status.is_success() {
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                return Err(eyre::eyre!(
                    "got HTTP {status} from {identity_uri}, server said: {error}"
                ));
            }

            let payload: String = res.text().await?;
            log::info!("Got Patreon response: {payload}");

            log::info!("Parsing Patreon JsonApiDocument from payload");
            let doc: jsonapi::model::DocumentData =
                match serde_json::from_str::<jsonapi::api::JsonApiDocument>(&payload)? {
                    jsonapi::api::JsonApiDocument::Data(doc) => {
                        log::info!("Successfully parsed JsonApiDocument as Data");
                        doc
                    }
                    jsonapi::api::JsonApiDocument::Error(errors) => {
                        log::info!("JsonApiDocument contains errors: {errors:?}");
                        return Err(eyre::eyre!("jsonapi errors: {errors:?}"));
                    }
                };

            log::info!("Extracting user from primary data");
            let user = match &doc.data {
                Some(jsonapi::api::PrimaryData::Single(user)) => {
                    log::info!("Found top-level user resource");
                    user
                }
                _ => {
                    log::info!("No top-level user resource found");
                    return Err(eyre::eyre!("no top-level user resource"));
                }
            };

            let mut tier_title = None;

            #[derive(Debug, serde::Deserialize)]
            struct UserAttributes {
                full_name: String,
                thumb_url: String,
            }
            log::info!("Getting user attributes");
            let user_attrs: UserAttributes = user.get_attributes()?;
            log::info!(
                "Found user attributes: full_name={}, thumb_url={}",
                user_attrs.full_name,
                user_attrs.thumb_url
            );

            log::info!("Getting user memberships");
            let memberships = user.get_multi_relationship(&doc, "memberships")?;
            log::info!("Found {} memberships", memberships.len());

            'each_membership: for (i, &membership) in memberships.iter().enumerate() {
                log::info!("Processing membership #{}", i + 1);

                let campaign = match membership.get_single_relationship(&doc, "campaign") {
                    Ok(campaign) => {
                        log::info!(
                            "Found campaign for membership #{}: id={}",
                            i + 1,
                            campaign.id
                        );
                        campaign
                    }
                    Err(e) => {
                        log::warn!("{e}, skipping campaign for membership #{}", i + 1);
                        continue;
                    }
                };

                let campaign_match = rc.patreon_campaign_ids.contains(&campaign.id);
                log::info!(
                    "Campaign {} is in our configured campaign_ids: {}",
                    campaign.id,
                    campaign_match
                );
                if !campaign_match {
                    log::info!(
                        "Skipping campaign {} (not in our configured list)",
                        campaign.id
                    );
                    continue;
                }

                let tiers =
                    match membership.get_multi_relationship(&doc, "currently_entitled_tiers") {
                        Ok(tiers) => {
                            log::info!("Found {} tiers for membership #{}", tiers.len(), i + 1);
                            tiers
                        }
                        Err(e) => {
                            log::warn!("{e}, skipping tiers for membership #{}", i + 1);
                            continue;
                        }
                    };

                if let Some(tier) = tiers.first() {
                    log::info!("Processing first tier: id={}", tier.id);

                    #[derive(Debug, serde::Deserialize)]
                    struct TierAttributes {
                        title: String,
                    }
                    let tier_attrs: TierAttributes = tier.get_attributes()?;
                    log::info!("Tier title: {}", tier_attrs.title);

                    tier_title = Some(tier_attrs.title);
                    log::info!(
                        "Found matching tier '{}' - breaking from membership loop",
                        tier_title.as_ref().unwrap()
                    );
                    break 'each_membership;
                } else {
                    log::info!("No tiers found for this membership");
                }
            }

            log::info!("Creating profile with patreon_id={}", user.id);
            let has_tier = tier_title.is_some();
            log::info!("User has tier from memberships: {has_tier}");

            let profile = PatreonProfile {
                id: PatreonUserId::new(user.id.clone()),
                tier: tier_title,
                full_name: user_attrs.full_name,
                avatar_url: Some(user_attrs.thumb_url),
            };

            log::info!("Refreshed user profile: {profile:3?}",);
            Ok(profile)
        })
    }

    /// List all sponsors using `credentials`, which must be the owner of the campaign ID
    /// for the given `RevisionConfig`.
    fn list_sponsors<'fut>(
        &'fut self,
        rc: &'fut RevisionConfig,
        client: &'fut dyn HttpClient,
        credentials: &'fut PatreonCredentials,
    ) -> BoxFuture<'fut, Result<Vec<PatreonProfile>>> {
        Box::pin(async move {
            // Check if credentials are expiring soon
            if credentials.expire_soon() {
                return Err(eyre::eyre!("Patreon credentials are expiring soon"));
            }

            let patreon_campaign_id = rc
                .patreon_campaign_ids
                .first()
                .expect("patreon_campaign_ids should have at least one element");

            let mut patrons: Vec<PatreonProfile> = Vec::new();

            let mut api_uri = Uri::builder()
                .scheme("https")
                .authority("www.patreon.com")
                .path_and_query(
                    libhttpclient::form_urlencoded::Serializer::new(format!(
                        "/api/oauth2/v2/campaigns/{patreon_campaign_id}/members?"
                    ))
                    .append_pair("include", "currently_entitled_tiers,user")
                    .append_pair("fields[member]", "full_name")
                    .append_pair("fields[user]", "thumb_url")
                    .append_pair("fields[tier]", "title")
                    .append_pair("page[size]", "50")
                    .finish(),
                )
                .build()
                .unwrap();

            let mut num_page = 0;
            loop {
                num_page += 1;
                log::info!("Fetching Patreon page {num_page}");
                log::debug!("Fetch uri: {api_uri}");

                let res = client
                    .get(api_uri.clone())
                    .bearer_auth(&credentials.access_token)
                    .polite_user_agent()
                    .send()
                    .await?;

                let status = res.status();
                if !status.is_success() {
                    let error = res
                        .text()
                        .await
                        .unwrap_or_else(|_| "Could not get error text".into());
                    return Err(eyre::eyre!(
                        "got HTTP {status} from {api_uri}, server said: {error}"
                    ));
                }

                let patreon_payload = res.text().await?;
                // std::fs::write("/tmp/patreon-payload.json", &patreon_payload)
                //     .wrap_err("Failed to write patreon payload to /tmp/patreon-payload.json")?;
                // eprintln!(
                //     "Wrote Patreon API response payload to /tmp/patreon-payload.json for debugging"
                // );

                let patreon_response: PatreonResponse = serde_json::from_str(&patreon_payload)?;
                let mut tiers_per_id: HashMap<String, Tier> = Default::default();
                let mut users_per_id: HashMap<String, User> = Default::default();

                for item in patreon_response.included {
                    match item {
                        Item::Tier(tier) => {
                            tiers_per_id.insert(tier.common.id.clone(), tier);
                        }
                        Item::User(user) => {
                            users_per_id.insert(user.common.id.clone(), user);
                        }
                        _ => {}
                    }
                }

                for item in patreon_response.data {
                    if let Item::Member(member) = item {
                        if let Some(full_name) = member.attributes.full_name.as_deref() {
                            let tier_title = if let Some(entitled) = member
                                .common
                                .relationships
                                .currently_entitled_tiers
                                .as_ref()
                            {
                                entitled.data.iter().find_map(|item_ref| {
                                    let ItemRef::Tier(tier_id) = item_ref;
                                    tiers_per_id
                                        .get(&tier_id.id)
                                        .and_then(|tier| tier.attributes.title.as_deref())
                                        .map(|title| title.to_string())
                                })
                            } else {
                                None
                            };

                            let mut thumb_url: Option<String> = None;
                            let user_id =
                                if let Some(user_rel) = member.common.relationships.user.as_ref() {
                                    let user_id = user_rel.data.id.clone();
                                    if let Some(user_item) = users_per_id.get(&user_id) {
                                        thumb_url = user_item.attributes.thumb_url.clone();
                                    }

                                    user_id
                                } else {
                                    continue;
                                };

                            let patron = PatreonProfile {
                                id: PatreonUserId::new(user_id.clone()),
                                tier: tier_title,
                                full_name: full_name.trim().to_string(),
                                avatar_url: thumb_url,
                            };
                            patrons.push(patron);
                        }
                    }
                }

                match patreon_response.links.and_then(|l| l.next) {
                    Some(next) => {
                        api_uri = match next.parse::<Uri>() {
                            Ok(uri) => uri,
                            Err(e) => return Err(eyre::eyre!("Failed to parse next URI: {}", e)),
                        };
                        continue;
                    }
                    None => break,
                }
            }

            Ok(patrons)
        })
    }
}

impl ModImpl {
    fn make_patreon_callback_url(&self, tc: &TenantConfig, web: WebConfig) -> String {
        let base_url = tc.web_base_url(web);
        let url = format!("{base_url}/login/patreon/callback");
        log::info!("Crafted patreon callback URL: {url}");
        url
    }
}

/// Patreon credentials as returned by the Patreon API
#[derive(Debug, Clone, Facet)]
struct PatreonCredentialsAPI {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u32,
    pub scope: String,
    #[facet(default)]
    pub token_type: Option<String>,
    #[facet(default)]
    pub version: Option<String>,
}

/// Patreon credentials as we want to expose them to the rest of home
#[derive(Debug, Clone, Facet)]
pub struct PatreonCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: OffsetDateTime,
}

impl PatreonCredentials {
    pub fn expire_soon(&self) -> bool {
        let now = OffsetDateTime::now_utc();
        let twenty_four_hours = time::Duration::hours(1);
        self.expires_at - now < twenty_four_hours
    }
}

pub fn test_patreon_renewal() -> bool {
    std::env::var("TEST_PATREON_RENEWAL").is_ok()
}

#[derive(Facet, Debug, Clone)]
pub struct PatreonCallbackArgs {
    pub raw_query: String,

    /// if we're linking this patreon account to an existing UserID, this is set
    #[facet(default)]
    pub logged_in_user_id: Option<UserId>,
}

#[derive(Debug, Clone, Facet)]
pub struct PatreonRefreshCredentialsArgs {
    pub patreon_id: String,
}
