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
use config_types::is_development;
use conflux::{CacheBuster, InputPathRef};
use content_type::ContentType;
use credentials::UserApiKey;
use cub_types::{CubReq, CubTenant};
use http::{
    StatusCode,
    header::{ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS},
};
use mom_types::VerifyApiKeyArgs;
use objectstore_types::ObjectStoreKey;
use owo_colors::OwoColorize;

pub(crate) fn web_routes() -> Router {
    Router::new()
        .nest("/tags", tags::tag_routes())
        .nest("/login", login::login_routes())
        .nest("/internal-api", internal_api::internal_api_routes())
        .nest("/api", api::public_api_routes())
        .route("/robots.txt", get(robots_txt))
        .route("/whoami", get(whoami))
        .route("/index.xml", get(atom_feed))
        .route("/extra-files/{*path}", get(extra_files))
        .route("/extras/{*path}", get(extras_git).post(extras_git))
        .route("/favicon.ico", get(favicon))
        .route("/", get(serve_page_route))
        .route("/{*path}", get(serve_page_route))
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

            log::info!("Opening editor {editor} for file {file}");

            tokio::spawn(async move {
                if let Err(e) = tokio::process::Command::new(editor)
                    .arg(file)
                    .status()
                    .await
                {
                    log::error!("Failed to open editor: {e}");
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

            log::warn!("{e}");
            return render_404(rx);
        }
    };

    let page = match irev.rev.pages.get_or_help(ResourceKind::Page, page_path) {
        Ok(page) => page.clone(),
        Err(e) => {
            log::error!("Failed to get page: {e}");
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
                log::info!("Adding draft_code to URL for easy sharing: {redirect_url}");
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
        log::info!("Redirecting to {redirect_target}");
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
        log::warn!("Unauthorized access attempt to extra files");
        return Err(LegacyHttpError::with_status(
            StatusCode::FORBIDDEN,
            "extra files are only available to Bronze sponsors and above",
        ));
    }

    if path.contains("..") {
        log::warn!("Path traversal attempt: {path}");
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
            log::warn!("Unsupported file type requested: {path}");
            return Err(LegacyHttpError::with_status(
                StatusCode::NOT_FOUND,
                "unsupported file type",
            ));
        }
    };

    let store = tr.tenant.store.clone();
    let key = ObjectStoreKey::new(format!("extra-files/{path}"));
    log::info!(
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

static GIT_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

fn git_client() -> &'static reqwest::Client {
    GIT_CLIENT.get_or_init(reqwest::Client::new)
}

async fn extras_git(
    axum::extract::Path(path): axum::extract::Path<String>,
    tr: CubReqImpl,
    req: Request,
) -> impl IntoResponse {
    use axum::body::to_bytes;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use cub_types::CubTenant;
    use http::Method;

    // Check for authorization header and validate JWT token
    let token = if let Some(auth_header) = req.headers().get(http::header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            extract_token_from_basic_auth(auth_str)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(api_key) = token {
        let api_key = UserApiKey::new(api_key);

        // Use mom tenant client to verify the API key and get tier
        let tcli = tr.tenant.tcli();

        match tcli.verify_api_key(&VerifyApiKeyArgs { api_key }).await {
            Ok(response) => {
                let tier = response.user_info.get_fasterthanlime_tier();
                log::info!("Valid API key for user with tier: {tier:?}");

                // Check if user has at least bronze tier
                if !tier.has_bronze() {
                    log::warn!("User does not have bronze tier access");
                    return (
                        StatusCode::FORBIDDEN,
                        [("WWW-Authenticate", "Basic realm=\"Git Access\"")],
                        "Bronze tier or higher required for git access",
                    )
                        .into_response();
                }
            }
            Err(e) => {
                log::warn!("Invalid API key: {e}");
                return (
                    StatusCode::UNAUTHORIZED,
                    [("WWW-Authenticate", "Basic realm=\"Git Access\"")],
                    "Invalid authentication token",
                )
                    .into_response();
            }
        }
    } else {
        log::warn!("No JWT token found in Authorization header or URL");
        return (
            StatusCode::UNAUTHORIZED,
            [("WWW-Authenticate", "Basic realm=\"Git Access\"")],
            "Authentication required",
        )
            .into_response();
    }

    // Get the query string, if any, and append to the target URL
    let original_uri = req.uri();
    let target_url = if let Some(query) = original_uri.query() {
        format!("https://code.bearcove.cloud/ftl-extras/{path}?{query}")
    } else {
        format!("https://code.bearcove.cloud/ftl-extras/{path}")
    };

    // Log incoming request details
    log::info!("Incoming request to /extras/{path}");
    log::info!("  Method: {}", req.method());
    log::info!("  URI: {}", req.uri());
    log::info!("  Headers:");
    for (name, value) in req.headers() {
        log::info!("    {}: {:?}", name.blue(), value.yellow());
    }

    // Clone headers before consuming the request
    let headers = req.headers().clone();
    let method = req.method().clone();

    // Determine the HTTP method
    let mut proxy_req = match method {
        Method::GET => git_client().get(&target_url),
        Method::POST => {
            // Read the body from the axum request
            let body = req.into_body();
            let body_bytes = match to_bytes(body, 10 * 1024 * 1024).await {
                Ok(b) => b,
                Err(e) => {
                    log::error!("Failed to read POST body: {e}");
                    return (StatusCode::BAD_REQUEST, format!("Failed to read body: {e}"))
                        .into_response();
                }
            };
            git_client().post(&target_url).body(body_bytes)
        }
        // If you want to support more HTTP methods, add match arms here.
        m => {
            log::warn!("Unsupported method for proxy: {m:?}");
            return (
                StatusCode::METHOD_NOT_ALLOWED,
                format!("Method {m} not supported"),
            )
                .into_response();
        }
    };

    // Forward headers from the original request, but replace authorization with git credentials
    let git_credentials = tr.tenant.tc().secrets.as_ref().and_then(|s| s.git.as_ref());

    for (header_name, header_value) in headers.iter() {
        if header_name == http::header::HOST {
            log::info!("  Overriding Host header to: code.bearcove.cloud");
            proxy_req = proxy_req.header(header_name, "code.bearcove.cloud");
            continue;
        }

        // Skip the original authorization header since we'll replace it with git credentials
        if header_name == http::header::AUTHORIZATION {
            log::info!("  Skipping original Authorization header");
            continue;
        }

        log::info!(
            "  Forwarding request header: {}: {:?}",
            header_name.to_string().blue(),
            header_value.to_str().unwrap_or("<binary>").yellow()
        );
        proxy_req = proxy_req.header(header_name, header_value);
    }

    // Add git credentials if available
    if let Some(git_creds) = git_credentials {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let auth_string = format!("{}:{}", git_creds.username, git_creds.password);
        let encoded = STANDARD.encode(&auth_string);
        let auth_header = format!("Basic {encoded}");
        proxy_req = proxy_req.header(http::header::AUTHORIZATION, auth_header);
        log::info!("  Added git credentials for upstream");
    } else {
        log::warn!("  No git credentials configured for this tenant");
    }

    log::info!("Proxying request to: {target_url}");

    match proxy_req.send().await {
        Ok(resp) => {
            let status = resp.status();
            log::info!("Response from upstream:");
            log::info!("  Status: {}", status.blue());
            log::info!("  Headers:");
            for (k, v) in resp.headers() {
                log::info!("    {}: {:?}", k.yellow(), v.green());
            }

            let mut headers = HeaderMap::new();
            // Denylist: don't forward hop-by-hop or sensitive headers.
            // See RFC 7230 section 6.1 and common hop-by-hop headers.
            const DENYLIST: &[&str] = &[];
            for (k, v) in resp.headers() {
                let k_str = k.as_str();
                if DENYLIST.iter().any(|deny| k_str.eq_ignore_ascii_case(deny)) {
                    log::info!(
                        "  Not forwarding denylisted header: {}: {:?}",
                        k.red(),
                        v.blue()
                    );
                    continue;
                }
                log::info!(
                    "  Forwarding response header: {}: {:?}",
                    k.green(),
                    v.blue()
                );
                headers.insert(k, v.clone());
            }

            // For git operations, we need to stream the response instead of buffering
            let body_stream = resp.bytes_stream();

            log::info!("Returning response with status: {status}");

            // Convert the stream to axum body
            use axum::body::Body;
            use futures_util::StreamExt;

            let body = Body::from_stream(body_stream.map(|result| {
                result.map_err(|e| {
                    log::error!("Error streaming proxy body: {e}");
                    std::io::Error::other(e)
                })
            }));

            (status, headers, body).into_response()
        }
        Err(e) => {
            log::error!("Proxy request failed: {e}");
            (StatusCode::BAD_GATEWAY, format!("Proxy error: {e}")).into_response()
        }
    }
}

fn extract_token_from_basic_auth(auth_header: &str) -> Option<String> {
    if let Some(basic_part) = auth_header.strip_prefix("Basic ") {
        use base64::{Engine, engine::general_purpose::STANDARD};
        if let Ok(decoded_bytes) = STANDARD.decode(basic_part) {
            if let Ok(decoded_str) = std::str::from_utf8(&decoded_bytes) {
                // Basic auth format is "username:password"
                // For our case, we expect the token to be in the password field
                if let Some((_, token)) = decoded_str.split_once(':') {
                    return Some(token.to_string());
                }
            }
        }
    }
    None
}
