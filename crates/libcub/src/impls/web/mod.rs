mod api;
mod internal_api;
mod login;
mod tags;

use std::net::SocketAddr;

use crate::impls::{
    cub_req::{CubReqImpl, RenderArgs},
    reply::{ClientCachePolicy, IntoLegacyReply, LegacyHttpError, LegacyReply},
};

use axum::{
    Router,
    extract::{ConnectInfo, Request},
    response::{IntoResponse, Redirect},
    routing::get,
};
use camino::Utf8PathBuf;
use closest::{GetOrHelp, ResourceKind};
use config::is_development;
use conflux::{CacheBuster, InputPathRef};
use content_type::ContentType;
use cub_types::{CubReq, CubTenant};
use http::{
    StatusCode,
    header::{ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS},
};
use objectstore::ObjectStoreKey;

pub(crate) fn web_routes() -> Router {
    Router::new()
        .nest("/tags", tags::tag_routes())
        .nest("/login", login::login_routes())
        .nest("/internal-api", internal_api::internal_api_routes())
        .nest("/api", api::public_api_routes())
        .route("/robots.txt", get(robots_txt))
        .route("/whoami", get(whoami))
        .route("/index.xml", get(atom_feed))
        .route("/extra-files/*path", get(extra_files))
        .route("/favicon.ico", get(favicon))
        .route("/", get(serve_page_route))
        .route("/*path", get(serve_page_route))
}

async fn robots_txt() -> &'static str {
    // don't tell robots anything for now
    ""
}

async fn atom_feed(tr: CubReqImpl) -> LegacyReply {
    tr.render(RenderArgs::new("index.xml").with_content_type(ContentType::Atom))
}

/// Render a 404 page using the template
pub(crate) fn render_404(tr: CubReqImpl) -> LegacyReply {
    let mut response = tr.render(RenderArgs::new("404.html"))?;
    *response.status_mut() = StatusCode::NOT_FOUND;
    Ok(response)
}

async fn serve_page_route(rx: CubReqImpl) -> LegacyReply {
    if rx.path.as_str() == "/dist/__open-in-editor" {
        if !is_development() {
            return Ok(StatusCode::NOT_FOUND.into_response());
        }

        if let Some(file) = rx.url_params_map().get("file").cloned() {
            let file = Utf8PathBuf::from(file);
            let file = rx.tenant_ref().ti().base_dir.join(file);
            let editor = "zed";

            tracing::info!("Opening editor {editor} for file {file}");

            tokio::spawn(async move {
                if let Err(e) = tokio::process::Command::new(editor)
                    .arg(file)
                    .status()
                    .await
                {
                    tracing::error!("Failed to open editor: {}", e);
                }
            });

            return Ok(StatusCode::OK.into_response());
        } else {
            return Ok(StatusCode::BAD_REQUEST.into_response());
        }
    }

    let irev = rx.tenant.rev()?;
    let page_route = &rx.path;
    let page_path = match irev
        .rev
        .page_routes
        .get_or_help(ResourceKind::Route, page_route)
    {
        Ok(path) => path,
        Err(e) => {
            if rx.path.as_str().ends_with(".png") {
                let cdn_base_url = &rx.tenant.tc().cdn_base_url(rx.web());
                let cdn_url = format!("{}{}", cdn_base_url, rx.path);
                return Ok(Redirect::to(&cdn_url).into_response());
            }

            tracing::warn!("{e}");
            return render_404(rx);
        }
    };

    let page = match irev.rev.pages.get_or_help(ResourceKind::Page, page_path) {
        Ok(page) => page.clone(),
        Err(e) => {
            tracing::error!("Failed to get page: {}", e);
            return render_404(rx);
        }
    };

    use crate::impls::access_control::CanAccess;
    use crate::impls::access_control::can_access;

    match can_access(&rx, &page)? {
        CanAccess::Yes(_) => {
            if page.draft
                && page.draft_code.is_some()
                && !rx.url_params_map().contains_key("draft_code")
            {
                // Admins can view drafts without the draft_code, but including it in the URL
                // makes it easier to share links directly from the browser's address bar
                let redirect_url = format!(
                    "{}?draft_code={}",
                    rx.path,
                    page.draft_code.as_ref().unwrap()
                );
                tracing::info!("Adding draft_code to URL for easy sharing: {redirect_url}");
                return Redirect::temporary(&redirect_url)
                    .into_response()
                    .into_legacy_reply();
            }
        }
        CanAccess::No(_) => { /* Access denied for non-admins, no redirect */ }
    }

    if &page.route != page_route {
        let redirect_target = if rx.raw_query().is_empty() {
            page.route.to_string()
        } else {
            format!("{}?{}", page.route, rx.raw_query())
        };
        tracing::info!("Redirecting to {redirect_target}");
        return Redirect::temporary(&redirect_target).into_legacy_reply();
    }

    let template_name = page.template.as_str();
    rx.render(RenderArgs::new(template_name).with_page(page))
}

async fn whoami(ConnectInfo(addr): ConnectInfo<SocketAddr>, req: Request) -> LegacyReply {
    let mut lines = vec![];
    lines.push(format!("RemoteAddr: {addr}"));
    lines.push(format!("GET {} {:?}", req.uri(), req.version()));
    for (name, value) in req.headers() {
        lines.push(format!("{name}: {value:?}"));
    }
    let response = lines.join("\n");
    Ok(response.into_response())
}

async fn extra_files(
    axum::extract::Path(path): axum::extract::Path<String>,
    tr: CubReqImpl,
) -> LegacyReply {
    let viewer = tr.viewer()?;
    if !(viewer.has_bronze || viewer.is_admin) {
        tracing::warn!("Unauthorized access attempt to extra files");
        return Err(LegacyHttpError::with_status(
            StatusCode::FORBIDDEN,
            "extra files are only available to Bronze sponsors and above",
        ));
    }

    if path.contains("..") {
        tracing::warn!("Path traversal attempt: {}", path);
        return Err(LegacyHttpError::with_status(
            StatusCode::BAD_REQUEST,
            "path traversal not allowed",
        ));
    }

    let content_type = match path.rsplit_once('.').map(|x| x.1) {
        Some("m4a") => ContentType::AAC,
        Some("ogg") => ContentType::OGG,
        Some("mp3") => ContentType::MP3,
        Some("flac") => ContentType::FLAC,
        _ => {
            tracing::warn!("Unsupported file type requested: {}", path);
            return Err(LegacyHttpError::with_status(
                StatusCode::NOT_FOUND,
                "unsupported file type",
            ));
        }
    };

    let store = tr.tenant.store.clone();
    let key = ObjectStoreKey::new(format!("extra-files/{path}"));
    tracing::info!(
        "Fetching object store key \x1b[33m{key}\x1b[0m for extra file \x1b[33m{path}\x1b[0m"
    );

    let res = store.get(&key).await?;
    let body = res.bytes().await?;

    Ok((
        StatusCode::OK,
        [
            (CONTENT_TYPE, content_type.as_str()),
            (
                ACCESS_CONTROL_ALLOW_ORIGIN,
                &tr.tenant.tc().web_base_url(tr.web()),
            ),
            (X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ClientCachePolicy::CacheBasicallyForever.to_header_tuple(),
        ],
        axum::body::Body::from(body),
    )
        .into_response())
}

async fn favicon(rcx: CubReqImpl) -> LegacyReply {
    let url = rcx
        .tenant_ref()
        .rev()?
        .rev
        .asset_url(rcx.web(), InputPathRef::from_str("/content/favicon.png"))?;
    Ok(Redirect::temporary(url.as_str()).into_response())
}
