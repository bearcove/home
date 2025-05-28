use autotrait::autotrait;
use config_types::{TenantConfig, StripeTierMapping};
use credentials::{Profile, Tier, UserInfo};
use eyre::Result;
use futures_core::future::BoxFuture;
use libhttpclient::{HttpClient, Uri};
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
            let stripe_secrets = match tc.secrets.as_ref().and_then(|s| s.stripe.as_ref()) {
                Some(config) => config,
                None => {
                    log::debug!("No Stripe configuration found for tenant");
                    return Ok(None);
                }
            };

            // Search for customer by email
            let customer = match search_customer_by_email(&client, &stripe_secrets.secret_key, email).await? {
                Some(customer) => customer,
                None => {
                    log::debug!("No Stripe customer found for email: {}", email);
                    return Ok(None);
                }
            };

            // Check for active subscriptions
            let tier = determine_tier_from_subscriptions(&customer.subscriptions, &stripe_secrets.tier_mapping);

            // Create user info
            let user_info = UserInfo {
                profile: Profile {
                    full_name: customer.name.unwrap_or_else(|| email.to_string()),
                    patreon_id: None,
                    github_id: None,
                    thumb_url: gravatar_url(email),
                },
                tier,
            };

            Ok(Some(user_info))
        })
    }
}

async fn search_customer_by_email(
    client: &Arc<dyn HttpClient>,
    api_key: &str,
    email: &str,
) -> Result<Option<StripeCustomer>> {
    let url = format!("https://api.stripe.com/v1/customers/search?query=email:'{}'&expand[]=data.subscriptions", email);
    let uri: Uri = url.parse()?;
    
    let response = client
        .get(uri)
        .bearer_auth(api_key)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        log::error!("Stripe API error: {}", error_text);
        return Ok(None);
    }

    let response_text = response.text().await?;
    let search_response: StripeCustomerSearchResponse = serde_json::from_str(&response_text)?;
    
    // Return the first customer if found
    Ok(search_response.data.into_iter().next())
}

fn determine_tier_from_subscriptions(
    subscriptions: &StripeSubscriptionList,
    tier_mapping: &StripeTierMapping,
) -> Option<Tier> {
    // Only consider active subscriptions
    let active_subs: Vec<_> = subscriptions.data.iter()
        .filter(|sub| sub.status == "active" || sub.status == "trialing")
        .collect();

    if active_subs.is_empty() {
        return None;
    }

    // Check for highest tier first (Gold > Silver > Bronze)
    for sub in &active_subs {
        for item in &sub.items.data {
            let price_id = &item.price.id;
            let product_id = &item.price.product;
            
            if tier_mapping.gold_ids.contains(price_id) || tier_mapping.gold_ids.contains(product_id) {
                return Some(Tier { title: "Gold".to_string() });
            }
        }
    }

    for sub in &active_subs {
        for item in &sub.items.data {
            let price_id = &item.price.id;
            let product_id = &item.price.product;
            
            if tier_mapping.silver_ids.contains(price_id) || tier_mapping.silver_ids.contains(product_id) {
                return Some(Tier { title: "Silver".to_string() });
            }
        }
    }

    for sub in &active_subs {
        for item in &sub.items.data {
            let price_id = &item.price.id;
            let product_id = &item.price.product;
            
            if tier_mapping.bronze_ids.contains(price_id) || tier_mapping.bronze_ids.contains(product_id) {
                return Some(Tier { title: "Bronze".to_string() });
            }
        }
    }

    // If they have an active subscription but it doesn't match any tier mapping,
    // default to Bronze
    Some(Tier { title: "Bronze".to_string() })
}

fn gravatar_url(email: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    email.to_lowercase().trim().hash(&mut hasher);
    format!("https://www.gravatar.com/avatar/{:x}?d=identicon&s=200", hasher.finish())
}