use autotrait::autotrait;
use config_types::{StripeTierMapping, TenantConfig};
use credentials::{Profile, Tier, UserInfo};
use eyre::Result;
use futures_core::future::BoxFuture;
use libhttpclient::{HeaderName, HeaderValue, HttpClient, Uri};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    &ModImpl
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeCustomer {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub subscriptions: StripeSubscriptionList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeSubscriptionList {
    pub data: Vec<StripeSubscription>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeSubscription {
    pub id: String,
    pub status: String,
    pub items: StripeSubscriptionItems,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeSubscriptionItems {
    pub data: Vec<StripeSubscriptionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeSubscriptionItem {
    pub price: StripePrice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripePrice {
    pub id: String,
    pub product: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeCustomerSearchResponse {
    pub data: Vec<StripeCustomer>,
}

#[autotrait]
impl Mod for ModImpl {
    fn lookup_user_by_email<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        client: Arc<dyn HttpClient>,
        email: &'fut str,
    ) -> BoxFuture<'fut, Result<Option<UserInfo>>> {
        Box::pin(async move {
            log::info!("Starting Stripe user lookup for email: {email}");

            let stripe_secrets = match tc.secrets.as_ref().and_then(|s| s.stripe.as_ref()) {
                Some(config) => {
                    log::debug!("Found Stripe configuration for tenant");
                    config
                }
                None => {
                    log::warn!("No Stripe configuration found for tenant");
                    return Ok(None);
                }
            };

            log::debug!(
                "Using Stripe tier mapping: gold={:?}, silver={:?}, bronze={:?}",
                stripe_secrets.tier_mapping.gold_ids,
                stripe_secrets.tier_mapping.silver_ids,
                stripe_secrets.tier_mapping.bronze_ids
            );

            // Search for customer by email
            let customer =
                match search_customer_by_email(&client, &stripe_secrets.secret_key, email).await? {
                    Some(customer) => {
                        log::info!(
                            "Found Stripe customer: id={}, email={}, name={:?}",
                            customer.id,
                            customer.email,
                            customer.name
                        );
                        log::debug!(
                            "Customer has {} subscriptions",
                            customer.subscriptions.data.len()
                        );
                        customer
                    }
                    None => {
                        log::info!("No Stripe customer found for email: {email}");
                        return Ok(None);
                    }
                };

            // Check for active subscriptions
            let tier = determine_tier_from_subscriptions(
                &customer.subscriptions,
                &stripe_secrets.tier_mapping,
            );

            log::info!("Determined tier for customer {}: {:?}", customer.id, tier);

            // Create user info
            let user_info = UserInfo {
                profile: Profile {
                    full_name: customer.name.unwrap_or_else(|| email.to_string()),
                    patreon_id: None,
                    github_id: None,
                    email: Some(email.to_string()),
                    thumb_url: gravatar_url(email),
                },
                tier,
            };

            log::info!(
                "Successfully created UserInfo for {}: tier={:?}",
                email,
                user_info.tier
            );

            Ok(Some(user_info))
        })
    }
}

async fn search_customer_by_email(
    client: &Arc<dyn HttpClient>,
    api_key: &str,
    email: &str,
) -> Result<Option<StripeCustomer>> {
    let url = format!(
        "https://api.stripe.com/v1/customers/search?query=email:'{email}'&expand[]=data.subscriptions",
    );
    log::debug!("Stripe API URL: {url}");
    let uri: Uri = url.parse()?;
    log::debug!("Making Stripe API request for email: {email}");
    let response = client
        .get(uri)
        .bearer_auth(api_key)
        .header(
            HeaderName::from_static("stripe-version"),
            HeaderValue::from_static("2020-08-27"),
        )
        .send()
        .await?;
    let status = response.status();
    log::debug!("Stripe API response status: {status}");

    if !status.is_success() {
        let error_text = response.text().await?;
        log::error!("Stripe API error (status {status}): {error_text}");
        // Parse the error to provide more context
        if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&error_text) {
            if let Some(error_obj) = error_json.get("error") {
                log::error!(
                    "Stripe error details: type={}, code={}, message={}",
                    error_obj
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                    error_obj
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                    error_obj
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
            }
        }

        return Ok(None);
    }
    let response_text = response.text().await?;
    log::trace!("Stripe API raw response: {response_text}");
    let search_response: StripeCustomerSearchResponse = match serde_json::from_str(&response_text) {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("Failed to parse Stripe response: {e}");
            log::error!("Response text was: {response_text}");
            return Err(e.into());
        }
    };

    log::info!(
        "Stripe search returned {} customers for email: {}",
        search_response.data.len(),
        email
    );

    // Return the first customer if found
    Ok(search_response.data.into_iter().next())
}

fn determine_tier_from_subscriptions(
    subscriptions: &StripeSubscriptionList,
    tier_mapping: &StripeTierMapping,
) -> Option<Tier> {
    log::debug!(
        "Determining tier from {} subscriptions",
        subscriptions.data.len()
    );

    // Only consider active subscriptions
    let active_subs: Vec<_> = subscriptions
        .data
        .iter()
        .filter(|sub| {
            let is_active = sub.status == "active" || sub.status == "trialing";
            log::debug!(
                "Subscription {}: status={}, active={}",
                sub.id,
                sub.status,
                is_active
            );
            is_active
        })
        .collect();

    log::info!(
        "Found {} active subscriptions out of {} total",
        active_subs.len(),
        subscriptions.data.len()
    );

    if active_subs.is_empty() {
        log::debug!("No active subscriptions found");
        return None;
    }

    // Check for highest tier first (Gold > Silver > Bronze)
    for sub in &active_subs {
        log::debug!("Checking subscription {} for Gold tier", sub.id);
        for item in &sub.items.data {
            let price_id = &item.price.id;
            let product_id = &item.price.product;
            log::trace!("  Checking item: price_id={price_id}, product_id={product_id}",);
            if tier_mapping.gold_ids.contains(price_id)
                || tier_mapping.gold_ids.contains(product_id)
            {
                log::info!("Found Gold tier match: price_id={price_id}, product_id={product_id}");
                return Some(Tier {
                    title: "Gold".to_string(),
                });
            }
        }
    }

    for sub in &active_subs {
        log::debug!("Checking subscription {} for Silver tier", sub.id);
        for item in &sub.items.data {
            let price_id = &item.price.id;
            let product_id = &item.price.product;
            log::trace!("  Checking item: price_id={price_id}, product_id={product_id}",);

            if tier_mapping.silver_ids.contains(price_id)
                || tier_mapping.silver_ids.contains(product_id)
            {
                log::info!("Found Silver tier match: price_id={price_id}, product_id={product_id}",);
                return Some(Tier {
                    title: "Silver".to_string(),
                });
            }
        }
    }

    for sub in &active_subs {
        log::debug!("Checking subscription {} for Bronze tier", sub.id);
        for item in &sub.items.data {
            let price_id = &item.price.id;
            let product_id = &item.price.product;
            log::trace!("  Checking item: price_id={price_id}, product_id={product_id}");

            if tier_mapping.bronze_ids.contains(price_id)
                || tier_mapping.bronze_ids.contains(product_id)
            {
                log::info!("Found Bronze tier match: price_id={price_id}, product_id={product_id}");
                return Some(Tier {
                    title: "Bronze".to_string(),
                });
            }
        }
    }

    // If they have an active subscription but it doesn't match any tier mapping,
    // default to Bronze
    log::warn!("Active subscription found but no tier mapping matched - defaulting to Bronze");
    log::warn!("Price/Product IDs in active subscriptions:");
    for sub in &active_subs {
        for item in &sub.items.data {
            log::warn!(
                "  - price_id={}, product_id={}",
                item.price.id,
                item.price.product
            );
        }
    }
    Some(Tier {
        title: "Bronze".to_string(),
    })
}

fn gravatar_url(email: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let normalized_email = email.to_lowercase().trim().to_string();
    log::trace!("Generating gravatar URL for email: {email} (normalized: {normalized_email})");

    let mut hasher = DefaultHasher::new();
    normalized_email.hash(&mut hasher);
    let hash = hasher.finish();

    let url = format!("https://www.gravatar.com/avatar/{hash:x}?d=identicon&s=200");
    log::trace!("Generated gravatar URL: {url}");

    url
}
