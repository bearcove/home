use config_types::Environment;
use credentials::AuthBundle;
use log::warn;
use tower_cookies::{Cookie, PrivateCookies, cookie::SameSite};

static COOKIE_NAME: &str = "home-credentials";

pub fn auth_bundle_as_cookie(ab: &AuthBundle) -> Cookie<'static> {
    let mut cookie = Cookie::new(COOKIE_NAME, facet_json::to_string(ab));
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

    let creds: AuthBundle = match facet_json::from_str(cookie.value()) {
        Ok(v) => v,
        Err(e) => {
            warn!("Got undeserializable cookie, removing: {e}");
            cookies.remove(cookie.clone().into_owned());
            return None;
        }
    };

    Some(creds)
}
