use crate::impls::{
    credentials::{AuthBundle, auth_bundle_as_cookie, auth_bundle_remove_cookie},
    cub_req::{CubReqImpl, RenderArgs},
    reply::{IntoLegacyReply, LegacyReply},
};
use axum::{Form, Router, response::Redirect, routing::get};
use cub_types::{CubReq, CubTenant};
use libgithub::GitHubLoginPurpose;
use libpatreon::PatreonCallbackArgs;
use serde::Deserialize;
use time::OffsetDateTime;
use tower_cookies::{Cookie, PrivateCookies};
use tracing::info;

pub(crate) fn login_routes() -> Router {
    Router::new()
        .route("/", get(serve_login))
        .route("/patreon", get(serve_login_with_patreon))
        .route("/patreon/callback", get(serve_patreon_callback))
        .route("/github", get(serve_login_with_github))
        .route("/github/callback", get(serve_github_callback))
        .route("/debug-credentials", get(serve_debug_credentials))
        .route("/logout", get(serve_logout))
}

#[derive(Deserialize)]
struct LoginParams {
    #[serde(default)]
    return_to: Option<String>,

    #[serde(default)]
    admin_login: bool,
}

async fn serve_login(tr: CubReqImpl, params: Option<Form<LoginParams>>) -> LegacyReply {
    let return_to: &str = params
        .as_ref()
        .and_then(|p| p.return_to.as_deref())
        .unwrap_or("");

    let mut args = RenderArgs::new("login.html").with_global("return_to", return_to);
    if let Some(params) = params {
        if let Some(return_to) = params.return_to.as_deref() {
            args = args.with_global("return_to", return_to);
        }
    }
    tr.render(args)
}

fn set_return_to_cookie(cookies: &PrivateCookies<'_>, params: &Option<Form<LoginParams>>) {
    if let Some(return_to) = params.as_ref().and_then(|p| p.return_to.as_deref()) {
        let mut cookie = Cookie::new("return_to", return_to.to_owned());
        cookie.set_path("/");
        cookie.set_expires(time::OffsetDateTime::now_utc() + time::Duration::minutes(30));
        cookies.add(cookie);
    }
}

async fn serve_login_with_patreon(
    tr: CubReqImpl,
    params: Option<Form<LoginParams>>,
) -> LegacyReply {
    tracing::info!("Initiating login with Patreon");
    set_return_to_cookie(&tr.cookies, &params);

    let patreon = libpatreon::load();
    let location = patreon.make_login_url(tr.web(), tr.tenant.tc())?;
    Redirect::to(&location).into_legacy_reply()
}

async fn serve_login_with_github(tr: CubReqImpl, params: Option<Form<LoginParams>>) -> LegacyReply {
    tracing::info!("Initiating login with GitHub");
    set_return_to_cookie(&tr.cookies, &params);

    let admin_login = params.as_ref().map(|p| p.admin_login).unwrap_or_default();
    let purpose = if admin_login {
        GitHubLoginPurpose::Admin
    } else {
        GitHubLoginPurpose::Regular
    };
    let location = libgithub::load().make_login_url(tr.tenant.tc(), tr.web(), purpose)?;
    Redirect::to(&location).into_legacy_reply()
}

async fn serve_patreon_callback(tr: CubReqImpl) -> LegacyReply {
    finish_login_callback(&tr, serve_patreon_callback_inner(&tr).await?).await
}

async fn finish_login_callback(tr: &CubReqImpl, auth_bundle: Option<AuthBundle>) -> LegacyReply {
    // if None, the oauth flow was cancelled
    if let Some(auth_bundle) = auth_bundle {
        let session_cookie = auth_bundle_as_cookie(&auth_bundle);
        tr.cookies.add(session_cookie);
        {
            let mut just_logged_in_cookie = Cookie::new("just_logged_in", "1");
            just_logged_in_cookie.set_path("/");
            // this is read by JavaScript to broadcast a `just_logged_in` event
            // via a BroadcastChannel
            tr.cookies.add(just_logged_in_cookie);
        }
    } else {
        tracing::info!("Login flow was cancelled (that's okay!)");
    }

    let location = tr.get_and_remove_return_to_cookie();
    Redirect::to(&location).into_legacy_reply()
}

async fn serve_patreon_callback_inner(tr: &CubReqImpl) -> eyre::Result<Option<AuthBundle>> {
    let tcli = tr.tenant.tcli();
    let callback_args = PatreonCallbackArgs {
        raw_query: tr.raw_query().to_owned().into(),
    };
    let res = tcli.patreon_callback(&callback_args).await?;
    Ok(res.map(|res| res.auth_bundle))
}

async fn serve_github_callback(tr: CubReqImpl) -> LegacyReply {
    let ts = tr.tenant.clone();
    let tcli = tr.tenant.tcli();
    let callback_args = libgithub::GitHubCallbackArgs {
        raw_query: tr.raw_query().to_owned(),
    };
    let callback_res = tcli.github_callback(&callback_args).await?;

    if let Some(callback_res) = callback_res.as_ref() {
        // if credentials are for creator and they don't have `read:org`, have them log in again
        let github_id = callback_res
            .auth_bundle
            .user_info
            .profile
            .github_id
            .as_deref()
            .unwrap_or_default();
        if ts.rc()?.admin_github_ids.iter().any(|id| id == github_id) {
            let mod_github = libgithub::load();
            if callback_res
                .github_credentials
                .scope
                .contains(&"read:org".to_owned())
            {
                info!("admin logged in, has read:org scope, continuing")
            } else {
                // we need that scope for the patron list
                info!("admin logged in, but missing read:org scope, redirecting to login page");
                let admin_login_url =
                    mod_github.make_login_url(&ts.ti.tc, tr.web(), GitHubLoginPurpose::Admin)?;
                return Redirect::to(&admin_login_url).into_legacy_reply();
            }
        }
    }

    finish_login_callback(&tr, callback_res.map(|res| res.auth_bundle)).await
}

async fn serve_logout(tr: CubReqImpl, return_to: Option<Form<LoginParams>>) -> LegacyReply {
    let return_to = match return_to.as_ref().and_then(|rt| rt.return_to.as_ref()) {
        // avoid open redirects by prepending `/` to the return_to URL
        Some(r) => format!("/{r}"),
        None => "/".into(),
    };

    // just in case, clear any `return_to` cookies as well (set on login)
    let mut return_to_cookie = Cookie::new("return_to", "");
    return_to_cookie.set_path("/");
    tr.cookies.add(return_to_cookie);

    tr.cookies.remove(auth_bundle_remove_cookie());

    let mut just_logged_out_cookie = Cookie::new("just_logged_out", "1");
    just_logged_out_cookie.set_path("/");
    tr.cookies.add(just_logged_out_cookie);

    Redirect::to(&return_to).into_legacy_reply()
}

pub(crate) async fn serve_debug_credentials(tr: CubReqImpl) -> LegacyReply {
    let creds = &tr.auth_bundle;

    let mut text = String::new();
    use std::fmt::Write;
    writeln!(
        &mut text,
        "Here are your current credentials:\n\n{creds:#?}"
    )
    .unwrap();
    if let Some(creds) = creds.as_ref() {
        let remaining = *creds.expires_at - OffsetDateTime::now_utc();
        writeln!(&mut text).unwrap();
        writeln!(
            &mut text,
            "They're still valid for {} seconds",
            remaining.whole_seconds()
        )
        .unwrap();
    }

    text.into_legacy_reply()
}
