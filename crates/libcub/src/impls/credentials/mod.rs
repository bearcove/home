use config_types::Environment;
use http::Uri;
use libpatreon::{PatreonRefreshCredentials, PatreonRefreshCredentialsArgs};
use merde::IntoStatic;
use time::OffsetDateTime;
use tower_cookies::{Cookie, PrivateCookies, cookie::SameSite};
use tracing::{debug, warn};

// Export these types for use in the crate
pub use credentials::AuthBundle;

use super::global_state;

static COOKIE_NAME: &str = "home-credentials";

pub fn auth_bundle_as_cookie(ab: &AuthBundle) -> Cookie<'static> {
    let mut cookie = Cookie::new(COOKIE_NAME, merde::json::to_string(ab).unwrap());
    auth_bundle_configure_cookie(&mut cookie);
    cookie.set_expires(Some(
        time::OffsetDateTime::now_utc() + time::Duration::days(31),
    ));
    cookie
}

pub fn auth_bundle_remove_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(COOKIE_NAME, "");
    auth_bundle_configure_cookie(&mut cookie);
    cookie
}

fn auth_bundle_configure_cookie(cookie: &mut Cookie) {
    if Environment::default().is_prod() {
        cookie.set_same_site(Some(SameSite::None));
        cookie.set_secure(true);
        cookie.set_http_only(true);
    }
    cookie.set_path("/");
}

pub async fn authbundle_load_from_cookies(cookies: &PrivateCookies<'_>) -> Option<AuthBundle> {
    let cookie = cookies.get(COOKIE_NAME)?;

    let creds: AuthBundle = match merde::json::from_str(cookie.value()) {
        Ok(v) => v,
        Err(e) => {
            warn!(?e, "Got undeserializable cookie, removing");
            cookies.remove(cookie.clone().into_owned());
            return None;
        }
    };

    let now = OffsetDateTime::now_utc();
    if now < *creds.expires_at {
        // credentials aren't expired yet
        return Some(creds.into_static());
    }

    debug!("Refreshing cookies");
    let creds = match refresh_credentials(&creds).await {
        Err(e) => {
            warn!("Refreshing credentials failed, will log out: {:?}", e);
            cookies.remove(cookie.clone().into_owned());
            return None;
        }
        Ok(creds) => creds,
    };

    cookies.add(auth_bundle_as_cookie(&creds));
    Some(creds.into_static())
}

async fn refresh_credentials(creds: &AuthBundle) -> eyre::Result<AuthBundle> {
    let patreon_id = creds
        .user_info
        .profile
        .patreon_id
        .as_deref()
        .ok_or_else(|| eyre::eyre!("Can only refresh patreon credentials"))?;
    refresh_patreon_credentials(patreon_id.to_string()).await
}

async fn refresh_patreon_credentials(patreon_id: String) -> eyre::Result<AuthBundle> {
    let client = libhttpclient::load().client();

    let mom_base_url = &global_state().config.mom_base_url;
    let res = client
        .post(Uri::try_from(&format!("{mom_base_url}/patreon/refresh-credentials")).unwrap())
        .json(&PatreonRefreshCredentialsArgs { patreon_id })?
        .send()
        .await?;

    let res = res
        .json::<PatreonRefreshCredentials>()
        .await
        .inspect_err(|e| warn!("Failed to refresh credentials: {e}"))?;

    Ok(res.auth_bundle.into_static())
}
