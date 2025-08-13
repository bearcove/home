use crate::impls::{
    credentials::{auth_bundle_as_cookie, auth_bundle_remove_cookie},
    cub_req::{CubReqImpl, RenderArgs},
    reply::{IntoLegacyReply, LegacyReply},
};
use axum::{Form, Router, response::Redirect, routing::get};
use config_types::is_development;
use credentials::{AuthBundle, GithubProfile, GithubUserId, PatreonProfile, UserId, UserInfo};
use cub_types::{CubReq, CubTenant};
use libgithub::GithubLoginPurpose;
use libpatreon::PatreonCallbackArgs;
use log::info;
use serde::Deserialize;
use time::OffsetDateTime;
use tower_cookies::{Cookie, PrivateCookies};

pub(crate) fn login_routes() -> Router {
    Router::new()
        .route("/", get(serve_login))
        .route("/for-dev", get(serve_login_for_dev))
        .route("/patreon", get(serve_login_with_patreon))
        .route("/patreon/callback", get(serve_patreon_callback))
        .route("/github", get(serve_login_with_github))
        .route("/github/callback", get(serve_github_callback))
        .route("/discord", get(serve_login_with_discord))
        .route("/discord/callback", get(serve_discord_callback))
        .route("/logout", get(serve_logout))
}

#[derive(Deserialize)]
struct LoginParams {
    #[serde(default)]
    return_to: Option<String>,

    #[serde(default)]
    admin_login: bool,
}

async fn serve_login(tr: CubReqImpl, params: Form<LoginParams>) -> LegacyReply {
    let return_to = params.return_to.as_deref().unwrap_or("");

    let mut args = RenderArgs::new("login.html").with_global("return_to", return_to);
    if let Some(return_to) = params.return_to.as_deref() {
        args = args.with_global("return_to", return_to);
    }
    tr.render(args)
}

fn set_return_to_cookie(cookies: &PrivateCookies<'_>, params: &Form<LoginParams>) {
    if let Some(return_to) = params.return_to.as_deref() {
        let mut cookie = Cookie::new("return_to", return_to.to_owned());
        cookie.set_path("/");
        cookie.set_expires(time::OffsetDateTime::now_utc() + time::Duration::minutes(30));
        cookies.add(cookie);
    }
}

async fn serve_login_with_patreon(tr: CubReqImpl, params: Form<LoginParams>) -> LegacyReply {
    log::info!("Initiating login with Patreon");
    set_return_to_cookie(&tr.cookies(), &params);

    let patreon = libpatreon::load();
    let location = patreon.make_login_url(tr.web(), tr.tenant.tc())?;
    Redirect::to(&location).into_legacy_reply()
}

async fn serve_login_with_github(tr: CubReqImpl, params: Form<LoginParams>) -> LegacyReply {
    log::info!("Initiating login with Github");
    set_return_to_cookie(&tr.cookies(), &params);

    let purpose = if params.admin_login {
        GithubLoginPurpose::Admin
    } else {
        GithubLoginPurpose::Regular
    };
    let location = libgithub::load().make_login_url(tr.tenant.tc(), tr.web(), purpose)?;
    Redirect::to(&location).into_legacy_reply()
}

async fn serve_login_with_discord(tr: CubReqImpl, params: Form<LoginParams>) -> LegacyReply {
    log::info!("Initiating login with Discord");
    set_return_to_cookie(&tr.cookies(), &params);

    let discord = libdiscord::load();
    let location = discord.make_login_url(tr.tenant.tc(), tr.web())?;
    Redirect::to(&location).into_legacy_reply()
}

async fn serve_patreon_callback(tr: CubReqImpl) -> LegacyReply {
    finish_login_callback(&tr, serve_patreon_callback_inner(&tr).await?).await
}

async fn finish_login_callback(tr: &CubReqImpl, user_info: Option<UserInfo>) -> LegacyReply {
    // if None, the oauth flow was cancelled
    if let Some(user_info) = user_info {
        let auth_bundle = AuthBundle { user_info };
        let session_cookie = auth_bundle_as_cookie(&auth_bundle);
        tr.cookies().add(session_cookie);
        {
            let mut just_logged_in_cookie = Cookie::new("just_logged_in", "1");
            just_logged_in_cookie.set_path("/");
            // this is read by JavaScript to broadcast a `just_logged_in` event
            // via a BroadcastChannel
            tr.cookies().add(just_logged_in_cookie);
        }
    } else {
        log::info!("Login flow was cancelled (that's okay!)");
    }

    let location = tr.get_and_remove_return_to_cookie();
    Redirect::to(&location).into_legacy_reply()
}

async fn serve_patreon_callback_inner(tr: &CubReqImpl) -> eyre::Result<Option<UserInfo>> {
    let tcli = tr.tenant.tcli();
    let callback_args = PatreonCallbackArgs {
        raw_query: tr.raw_query().to_owned(),
        logged_in_user_id: tr.auth_bundle.as_ref().map(|ab| ab.user_info.id.clone()),
    };
    let res = tcli.patreon_callback(&callback_args).await?;
    Ok(res.map(|res| res.user_info))
}

async fn serve_github_callback(tr: CubReqImpl) -> LegacyReply {
    let ts = tr.tenant.clone();
    let tcli = tr.tenant.tcli();
    let callback_args = libgithub::GithubCallbackArgs {
        raw_query: tr.raw_query().to_owned(),
        logged_in_user_id: tr.auth_bundle.as_ref().map(|ab| ab.user_info.id.clone()),
    };
    let callback_res = tcli.github_callback(&callback_args).await?;

    if let Some(callback_res) = callback_res.as_ref() {
        // if credentials are for creator and they don't have `read:org`, have them log in again
        let github_id = callback_res
            .user_info
            .github
            .as_ref()
            .map(|gp| gp.id.clone())
            .unwrap_or_else(|| GithubUserId::new("weird".to_string()));
        if ts.rc()?.admin_github_ids.iter().any(|id| id == &github_id) {
            let mod_github = libgithub::load();
            if callback_res.scope.contains(&"read:org".to_owned()) {
                info!("admin logged in, has read:org scope, continuing")
            } else {
                // we need that scope for the patron list
                info!("admin logged in, but missing read:org scope, redirecting to login page");
                let admin_login_url =
                    mod_github.make_login_url(&ts.ti.tc, tr.web(), GithubLoginPurpose::Admin)?;
                return Redirect::to(&admin_login_url).into_legacy_reply();
            }
        }
    }

    finish_login_callback(&tr, callback_res.map(|res| res.user_info)).await
}

async fn serve_discord_callback(tr: CubReqImpl) -> LegacyReply {
    finish_login_callback(&tr, serve_discord_callback_inner(&tr).await?).await
}

async fn serve_discord_callback_inner(tr: &CubReqImpl) -> eyre::Result<Option<UserInfo>> {
    let tcli = tr.tenant.tcli();
    let callback_args = libdiscord::DiscordCallbackArgs {
        raw_query: tr.raw_query().to_owned(),
        logged_in_user_id: tr.auth_bundle.as_ref().map(|ab| ab.user_info.id.clone()),
    };
    let res = tcli.discord_callback(&callback_args).await?;
    Ok(res.map(|res| res.user_info))
}

async fn serve_logout(tr: CubReqImpl, return_to: Form<LoginParams>) -> LegacyReply {
    let return_to = match &return_to.return_to {
        // avoid open redirects by prepending `/` to the return_to URL
        Some(r) => format!("/{r}"),
        None => "/".into(),
    };

    // just in case, clear any `return_to` cookies as well (set on login)
    let mut return_to_cookie = Cookie::new("return_to", "");
    return_to_cookie.set_path("/");
    tr.cookies().add(return_to_cookie);

    tr.cookies().remove(auth_bundle_remove_cookie());

    let mut just_logged_out_cookie = Cookie::new("just_logged_out", "1");
    just_logged_out_cookie.set_path("/");
    tr.cookies().add(just_logged_out_cookie);

    Redirect::to(&return_to).into_legacy_reply()
}

async fn serve_login_for_dev(tr: CubReqImpl) -> LegacyReply {
    if !is_development() {
        // we'd return a 404 but this is open-source so.. feels unnecessary
        return axum::http::StatusCode::UNAUTHORIZED.into_legacy_reply();
    }

    let rev = tr.tenant.rev()?;
    let patreon_id = rev.rev.pak.rc.admin_patreon_ids.first().cloned();
    let github_id = rev.rev.pak.rc.admin_github_ids.first().cloned();

    let user_info = UserInfo {
        id: UserId::new("1".to_string()),
        patreon: patreon_id.map(|id| PatreonProfile {
            id,
            tier: Some("dev".to_string()),
            full_name: "Dev User".to_string(),
            avatar_url: Some("https://placehold.co/32".to_string()),
        }),
        github: github_id.map(|id| GithubProfile {
            id,
            monthly_usd: Some(0),
            sponsorship_privacy_level: Some("PRIVATE".to_string()),
            name: Some("Dev User".to_string()),
            login: "devuser".to_string(),
            avatar_url: Some("https://placehold.co/32".to_string()),
        }),
        discord: None,
        fetched_at: OffsetDateTime::now_utc(),
    };

    let auth_bundle = AuthBundle { user_info };

    let session_cookie = auth_bundle_as_cookie(&auth_bundle);
    tr.cookies().add(session_cookie);
    {
        let mut just_logged_in_cookie = Cookie::new("just_logged_in", "1");
        just_logged_in_cookie.set_path("/");
        tr.cookies().add(just_logged_in_cookie);
    }

    // Don't use return_to for dev login, just go home
    Redirect::to("/").into_legacy_reply()
}
