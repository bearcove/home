use crate::impls::{
    access_control::{CanAccess, can_access},
    global_state,
    reply::{IntoLegacyReply, LegacyReply},
};
use axum::{
    body::Bytes,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use config_types::{Environment, WebConfig};
use conflux::{CacheBuster, InputPathRef, LoadedPage, Route, Viewer};
use content_type::ContentType;
use credentials::AuthBundle;
use cub_types::{CubReq, CubTenant};
use eyre::Result;
use futures_core::future::BoxFuture;
use hattip::{HBody, HError, HReply};
use http::{Uri, header, request};
use libwebsock::WebSocketStream;
use std::{sync::Arc, time::Instant};
use template_types::{DataObject, DataValue, RenderTemplateArgs};
use tower_cookies::Cookies;
use url::form_urlencoded;

use super::CubTenantImpl;

/// Allows rendering jinja templates (via minjinja)
/// Actually turned into "what is extracted from requests",
/// for example it has the tenant state
#[derive(Clone)]
pub struct CubReqImpl {
    pub(crate) cookie_key: tower_cookies::Key,
    pub(crate) public_cookies: Cookies,

    pub tenant: Arc<CubTenantImpl>,
    pub path: Route,
    pub auth_bundle: Option<AuthBundle>,
    pub(crate) viewer: Viewer,
    pub parts: request::Parts,
}

impl std::fmt::Debug for CubReqImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CubReqImpl").finish_non_exhaustive()
    }
}

impl<S> FromRequestParts<S> for CubReqImpl
where
    S: Send + Sync + 'static,
{
    type Rejection = LegacyReply;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Simply extract CubReqImpl from extensions - it should have been inserted by CubReqLayer
        parts.extensions.remove::<CubReqImpl>().ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "CubReqImpl not found in extensions - ensure CubReqLayer is installed",
            )
                .into_legacy_reply()
        })
    }
}

pub struct RenderArgs {
    pub(crate) template_name: String,
    pub(crate) page: Option<Arc<LoadedPage>>,
    pub(crate) additional_globals: DataObject,
    pub(crate) content_type: ContentType,
}

impl RenderArgs {
    pub fn new(template_name: impl Into<String>) -> Self {
        let template_name: String = template_name.into();
        let content_type =
            ContentType::guess_from_path(&template_name).unwrap_or(ContentType::HTML);
        Self {
            template_name,
            page: None,
            additional_globals: Default::default(),
            content_type,
        }
    }

    pub fn with_page(mut self, page: Arc<LoadedPage>) -> Self {
        self = self.with_global("title", page.title.clone());
        self.page = Some(page);
        self
    }

    pub fn with_content_type(mut self, content_type: ContentType) -> Self {
        self.content_type = content_type;
        self
    }

    pub fn with_global(mut self, key: impl Into<String>, value: impl Into<DataValue>) -> Self {
        self.additional_globals.insert(key.into(), value.into());
        self
    }
}

impl CubReqImpl {
    pub fn raw_query(&self) -> &str {
        self.parts.uri.query().unwrap_or_default()
    }

    pub fn render(&self, args: RenderArgs) -> LegacyReply {
        if let Some(page) = args.page.as_ref() {
            let access = can_access(self, page);
            log::debug!("\x1b[1;32m{}\x1b[0m {access:?}", page.route);

            if matches!(access, CanAccess::No(_)) {
                return self.render_inner(RenderArgs::new("404.html"));
            }
        }

        self.render_inner(args)
    }

    fn render_inner(&self, args: RenderArgs) -> LegacyReply {
        let start = Instant::now();
        let template_name = &args.template_name;

        let auth_bundle = &self.auth_bundle;
        let irev = self.tenant.rev()?;
        let templates = self.tenant.templates()?;

        let mut buffer: Vec<u8> = Default::default();
        templates.render_template_to(
            &mut buffer,
            RenderTemplateArgs {
                template_name,
                path: self.path.clone(),
                raw_query: self.raw_query(),
                user_info: auth_bundle.as_ref().map(|creds| creds.user_info.clone()),
                page: args.page.clone(),
                additional_globals: args.additional_globals,
                rv: irev.rev.clone(),
                index: self.tenant.index()?,
                gv: self.tenant.clone(),
                web: self.web(),
            },
        )?;
        let rendered = String::from_utf8(buffer)?;
        let env = Environment::default();
        let web = global_state().web;

        let prefix = "<!-- inserted by home -->\n";
        // TODO: a bunch of this could be cached
        let head_insert = match env {
            Environment::Development => {
                format!(
                    "{}<script type=\"module\" src=\"{}/dist/src/bundle.ts\"></script>",
                    prefix,
                    self.tenant_ref().tc().cdn_base_url(web)
                )
            }
            Environment::Production => {
                let bundle_js_url = irev
                    .rev
                    .asset_url(web, InputPathRef::from_str("/dist/assets/bundle.js"))?;
                let bundle_css_url = irev
                    .rev
                    .asset_url(web, InputPathRef::from_str("/dist/assets/bundle.css"))?;
                format!(
                    "{prefix}<script type=\"module\" src=\"{bundle_js_url}\" defer></script><link rel=\"stylesheet\" href=\"{bundle_css_url}\" media=\"print\" onload=\"this.media='all'\">"
                )
            }
        };
        let head_insert = if let Some(page) = args.page.as_ref() {
            format!(
                "<meta property=\"home:page-path\" content=\"{}\">{}",
                page.path, head_insert
            )
        } else {
            head_insert
        };

        let rendered = if let Some(head_end_index) = rendered.find("</head>") {
            format!(
                "{}{}{}",
                &rendered[..head_end_index],
                head_insert,
                &rendered[head_end_index..]
            )
        } else {
            log::warn!("Unable to find </head> tag in rendered content. Head insert not applied.");
            rendered
        };
        log::debug!(
            "Done rendering: template_name = {:?}, elapsed = {:?}",
            template_name,
            start.elapsed()
        );

        let body = Bytes::from(rendered);
        let response = (
            StatusCode::OK,
            [
                (header::CACHE_CONTROL, "no-cache"),
                (header::CONTENT_TYPE, args.content_type.as_str()),
            ],
            body,
        )
            .into_legacy_reply()?;
        Ok(response)
    }

    /// Get the value of the `return_to` cookie and remove it from the cookie jar
    pub fn get_and_remove_return_to_cookie(&self) -> String {
        let mut value = "".to_owned();
        if let Some(cookie) = self.cookies().get("return_to") {
            // security: prepending `/` protects against crafting URLs that would
            // redirect to different websites (an open redirect)
            value = format!("/{}", cookie.value());
            self.cookies().remove(cookie);
        }
        value
    }

    pub fn cookies(&self) -> tower_cookies::PrivateCookies<'_> {
        self.public_cookies.private(&self.cookie_key)
    }
}

impl CubReq for CubReqImpl {
    fn web(&self) -> WebConfig {
        global_state().web
    }

    fn route(&self) -> &conflux::RouteRef {
        &self.path
    }

    fn parts(&self) -> &Parts {
        &self.parts
    }

    fn uri(&self) -> &Uri {
        &self.parts.uri
    }

    fn url_params(&self) -> Vec<(String, String)> {
        form_urlencoded::parse(self.raw_query().as_bytes())
            .into_owned()
            .collect()
    }

    /// Borrows the tenant
    fn tenant_ref(&self) -> &dyn CubTenant {
        self.tenant.as_ref()
    }

    /// Clones a handle the tenant
    fn tenant_owned(&self) -> Arc<dyn CubTenant> {
        self.tenant.clone()
    }

    fn has_ws(&self) -> bool {
        self.parts
            .extensions
            .get::<hyper::upgrade::OnUpgrade>()
            .is_some()
    }

    fn on_ws_upgrade(
        mut self: Box<Self>,
        on_upgrade: Box<dyn FnOnce(Box<dyn WebSocketStream>) + Send + Sync + 'static>,
    ) -> BoxFuture<'static, HReply> {
        Box::pin(async move {
            let upgrade =
                match axum::extract::ws::WebSocketUpgrade::from_request_parts(&mut self.parts, &())
                    .await
                {
                    Ok(onup) => onup,
                    Err(e) => {
                        log::warn!("Failed to upgrade to WebSocket: {e}");
                        return Err(HError::Internal {
                            err: "failed websocket upgrade".into(),
                        });
                    }
                };
            // websocket upgrades have empty bodies anyway
            let res = upgrade
                .on_upgrade(|ws| async move { on_upgrade(Box::new(WsWrapper(ws))) })
                .map(|_old_body| HBody::empty());
            Ok(res)
        })
    }

    fn reddit_secrets(&self) -> eyre::Result<&config_types::RedditSecrets> {
        global_state()
            .config
            .reddit_secrets
            .as_ref()
            .ok_or_else(|| eyre::eyre!("reddit secrets not found"))
    }
}

/// Compatibility wrapper between axum and libwebsock (tungstenite)
struct WsWrapper(axum::extract::ws::WebSocket);

impl WebSocketStream for WsWrapper {
    fn send(&mut self, frame: libwebsock::Message) -> BoxFuture<'_, eyre::Result<()>> {
        Box::pin(async move {
            use axum::extract::ws;
            let msg = match frame {
                libwebsock::Message::Text(text) => ws::Message::text(text.as_str()),
                libwebsock::Message::Binary(bytes) => ws::Message::binary(bytes),
                libwebsock::Message::Ping(data) => ws::Message::Ping(data),
                libwebsock::Message::Pong(data) => ws::Message::Pong(data),
                libwebsock::Message::Close(frame) => {
                    ws::Message::Close(frame.map(|f| ws::CloseFrame {
                        code: f.code.into(),
                        reason: f.reason.as_str().into(),
                    }))
                }
                _ => {
                    // ignore Frame messages
                    return Ok(());
                }
            };

            self.0
                .send(msg)
                .await
                .map_err(|e| eyre::eyre!("WebSocket send error: {}", e))?;

            Ok(())
        })
    }

    fn send_binary(&mut self, msg: Bytes) -> BoxFuture<'_, eyre::Result<()>> {
        Box::pin(async move {
            self.0
                .send(axum::extract::ws::Message::binary(msg))
                .await
                .map_err(|e| eyre::eyre!("WebSocket send binary error: {}", e))?;
            Ok(())
        })
    }

    fn send_text(&mut self, msg: String) -> BoxFuture<'_, eyre::Result<()>> {
        Box::pin(async move {
            self.0
                .send(axum::extract::ws::Message::text(msg))
                .await
                .map_err(|e| eyre::eyre!("WebSocket send text error: {}", e))?;
            Ok(())
        })
    }

    fn receive(&mut self) -> BoxFuture<'_, Option<eyre::Result<libwebsock::Message>>> {
        Box::pin(async move {
            use axum::extract::ws;
            let res = match self.0.recv().await? {
                Ok(msg) => {
                    let frame = match msg {
                        ws::Message::Text(text) => libwebsock::Message::Text(text.as_str().into()),
                        ws::Message::Binary(bytes) => libwebsock::Message::Binary(bytes),
                        ws::Message::Ping(bytes) => libwebsock::Message::Ping(bytes),
                        ws::Message::Pong(bytes) => libwebsock::Message::Pong(bytes),
                        ws::Message::Close(frame) => {
                            // axum's CloseFrame is tungstenite::protocol::CloseFrame
                            // libwebsock::CloseFrame needs to be constructed from tungstenite's CloseFrame
                            libwebsock::Message::Close(frame.map(|f| libwebsock::CloseFrame {
                                code: f.code.into(),
                                reason: f.reason.as_str().into(),
                            }))
                        }
                    };
                    Ok(frame)
                }
                Err(e) => Err(eyre::eyre!("WebSocket receive error: {}", e)),
            };
            Some(res)
        })
    }
}
