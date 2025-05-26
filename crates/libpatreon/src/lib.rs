use autotrait::autotrait;
use config_types::{RevisionConfig, TenantConfig, WebConfig};
use credentials::AuthBundle;
use eyre::Context as _;
use eyre::Result;
use facet::Facet;
use futures_core::future::BoxFuture;
use libhttpclient::{HttpClient, Uri};
use std::collections::HashSet;
use url::Url;

use std::collections::HashMap;

mod jsonapi_ext;
use jsonapi_ext::*;

mod model;
use model::*;

fn patreon_refresh_interval() -> time::Duration {
    if test_patreon_renewal() {
        return time::Duration::seconds(10);
    }
    time::Duration::days(1)
}

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

    fn handle_oauth_callback<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        web: WebConfig,
        args: &'fut PatreonCallbackArgs,
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

            let res = libhttpclient::load()
                .client()
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

            let creds = res.json::<PatreonCredentials>().await?;
            log::info!(
                "Successfully obtained Patreon token with scope {}",
                &creds.scope
            );
            Ok(Some(creds))
        })
    }

    fn to_auth_bundle<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        rc: &'fut RevisionConfig,
        creds: PatreonCredentials,
        store: &'fut dyn PatreonStore,
        mode: ForcePatreonRefresh,
    ) -> BoxFuture<'fut, Result<(PatreonCredentials, AuthBundle)>> {
        Box::pin(async move {
            let res = match mode {
                ForcePatreonRefresh::DontForceRefresh => self.to_auth_bundle_once(rc, &creds).await,
                ForcePatreonRefresh::ForceRefresh => Err(eyre::eyre!("must refresh")),
            };

            match res {
                Ok(auth_bundle) => Ok((creds, auth_bundle)),
                Err(e) => {
                    log::warn!("Couldn't get user profile, will refresh: {e}");

                    let tok_params = {
                        let patreon_secrets = tc.patreon_secrets()?;

                        url::form_urlencoded::Serializer::new(String::new())
                            .append_pair("grant_type", "refresh_token")
                            .append_pair("refresh_token", &creds.refresh_token)
                            .append_pair("client_id", &patreon_secrets.oauth_client_id)
                            .append_pair("client_secret", &patreon_secrets.oauth_client_secret)
                            .finish()
                    };
                    let client = libhttpclient::load().client();
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
                    let site_creds = self.to_auth_bundle_once(rc, &pat_creds).await?;

                    let patreon_id = site_creds.user_info.profile.patreon_id()?;
                    store.save_patreon_credentials(patreon_id, &pat_creds)?;

                    Ok((pat_creds, site_creds))
                }
            }
        })
    }

    fn list_sponsors<'fut>(
        &'fut self,
        rc: &'fut RevisionConfig,
        client: &'fut dyn HttpClient,
        store: &'fut dyn PatreonStore,
    ) -> BoxFuture<'fut, Result<HashSet<String>>> {
        Box::pin(async move {
            let patreon_campaign_id = rc
                .patreon_campaign_ids
                .first()
                .expect("patreon_campaign_ids should have at least one element");
            let creator_patreon_user_id = rc.admin_patreon_ids.first().expect(
                    "admin_patreon_ids should have at least one element (whoever the campaign belongs to)",
                );

            let patreon_creds = store
                .fetch_patreon_credentials(creator_patreon_user_id)?
                .ok_or_else(|| eyre::eyre!("creator needs to log in with Patreon first"))?;

            let mut credited_patrons: HashSet<String> = Default::default();

            let credited_tiers: HashSet<String> = ["Silver", "Gold"]
                .into_iter()
                .map(|x| x.to_string())
                .collect();

            let mut api_uri = Uri::builder()
                .scheme("https")
                .authority("www.patreon.com")
                .path_and_query(
                    libhttpclient::form_urlencoded::Serializer::new(format!(
                        "/api/oauth2/v2/campaigns/{patreon_campaign_id}/members?"
                    ))
                    .append_pair("include", "currently_entitled_tiers")
                    .append_pair("fields[member]", "full_name")
                    .append_pair("fields[tier]", "title")
                    .append_pair("page[size]", "100")
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
                    .bearer_auth(&patreon_creds.access_token)
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
                let patreon_response: PatreonResponse = serde_json::from_str(&patreon_payload)?;

                let mut tiers_per_id: HashMap<String, Tier> = Default::default();
                for tier in patreon_response.included {
                    if let Item::Tier(tier) = tier {
                        tiers_per_id.insert(tier.common.id.clone(), tier);
                    }
                }

                for item in patreon_response.data {
                    if let Item::Member(member) = item {
                        if let Some(full_name) = member.attributes.full_name.as_deref() {
                            if let Some(entitled) = member.rel("currently_entitled_tiers") {
                                for item_ref in entitled.data.iter() {
                                    let ItemRef::Tier(tier_id) = item_ref;
                                    if let Some(tier) = tiers_per_id.get(&tier_id.id) {
                                        if let Some(title) = tier.attributes.title.as_deref() {
                                            if credited_tiers.contains(title) {
                                                credited_patrons
                                                    .insert(full_name.trim().to_string());
                                            } else {
                                                log::trace!("Tier {title} not credited");
                                            }
                                        }
                                    } else {
                                        log::trace!("Tier for id {} not found", tier_id.id);
                                    }
                                }
                            } else {
                                log::trace!("No currently_entitled_tiers for member: {full_name}");
                            }
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

            Ok(credited_patrons)
        })
    }
}

impl ModImpl {
    fn make_patreon_callback_url(&self, tc: &TenantConfig, web: WebConfig) -> String {
        let base_url = tc.web_base_url(web);
        format!("{base_url}/login/patreon/callback")
    }

    async fn to_auth_bundle_once(
        &self,
        rc: &RevisionConfig,
        creds: &PatreonCredentials,
    ) -> Result<AuthBundle> {
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
        let res = libhttpclient::load()
            .client()
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

            let tiers = match membership.get_multi_relationship(&doc, "currently_entitled_tiers") {
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
        let profile = credentials::Profile {
            patreon_id: Some(user.id.clone()),
            github_id: None,
            full_name: user_attrs.full_name,
            thumb_url: user_attrs.thumb_url,
        };
        let has_tier = tier_title.is_some();
        log::info!("User has tier from memberships: {has_tier}");

        let is_admin = rc.admin_patreon_ids.contains(&user.id);
        log::info!("User is in admin_patreon_ids list: {is_admin}");
        let tier_title = if has_tier {
            log::info!("Using tier from membership: {tier_title:?}");
            tier_title
        } else if is_admin {
            let creator_tier = creator_tier_name();
            log::info!("User is admin, using creator tier: {creator_tier:?}");
            creator_tier
        } else {
            log::info!("User has no tier and is not admin");
            None
        };

        log::info!(
            "Patreon user \x1b[32m{:?}\x1b[0m logged in (ID: \x1b[33m{:?}\x1b[0m, tier: \x1b[36m{:?}\x1b[0m)",
            profile.full_name,
            user.id,
            tier_title
        );

        let user_info = credentials::UserInfo {
            profile,
            tier: tier_title.map(|title| credentials::Tier { title }),
        };

        let auth_bundle = credentials::AuthBundle {
            expires_at: time::OffsetDateTime::now_utc() + patreon_refresh_interval(),
            user_info,
        };
        Ok(auth_bundle)
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

#[derive(Debug, Clone, Facet)]
pub struct PatreonCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u32,
    pub scope: String,
    #[facet(default)]
    pub token_type: Option<String>,
    #[facet(default)]
    pub version: Option<String>,
}

pub fn test_patreon_renewal() -> bool {
    std::env::var("TEST_PATREON_RENEWAL").is_ok()
}

#[derive(Debug, Clone, Copy)]
pub enum ForcePatreonRefresh {
    DontForceRefresh,
    ForceRefresh,
}

pub trait PatreonStore: Send + Sync + 'static {
    fn fetch_patreon_credentials(&self, patreon_id: &str) -> Result<Option<PatreonCredentials>>;

    fn save_patreon_credentials(
        &self,
        patreon_id: &str,
        credentials: &PatreonCredentials,
    ) -> Result<()>;
}

#[derive(Facet, Debug, Clone)]
pub struct PatreonCallbackArgs {
    pub raw_query: String,
}

#[derive(Debug, Clone, Facet)]
pub struct PatreonCallbackResponse {
    pub auth_bundle: AuthBundle,
}

#[derive(Debug, Clone, Facet)]
pub struct PatreonRefreshCredentialsArgs {
    pub patreon_id: String,
}

#[derive(Debug, Clone, Facet)]
pub struct PatreonRefreshCredentials {
    pub auth_bundle: AuthBundle,
}
