use std::task::{Context, Poll};

use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode, header, request::Parts},
    response::IntoResponse as _,
};
use futures_core::future::BoxFuture;
use tower::{Layer, Service};
use tower_cookies::Cookies;
use url::form_urlencoded;

use crate::impls::{
    credentials::authbundle_load_from_cookies,
    cub_req::CubReqImpl,
    global_state::global_state,
    host_extract,
    reply::{IntoLegacyReply, LegacyReply},
    types::DomainResolution,
};
use axum::extract::FromRequestParts;
use config_types::Environment;
use conflux::Route;
use credentials::{AuthBundle, UserApiKey};
use cub_types::CubTenant;
use mom_types::VerifyApiKeyArgs;

/// Layer that extracts CubReqImpl and inserts it as an extension into the request
#[derive(Clone)]
pub(crate) struct CubReqLayer;

impl<S> Layer<S> for CubReqLayer {
    type Service = CubReqService<S>;

    fn layer(&self, service: S) -> Self::Service {
        CubReqService { inner: service }
    }
}

#[derive(Clone)]
pub(crate) struct CubReqService<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for CubReqService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + Clone + 'static,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let inner = self.inner.clone();

        Box::pin(async move {
            // Extract the parts we need
            let (mut parts, body) = req.into_parts();

            // Try to create CubReqImpl from the request parts
            match create_cub_req_impl(&mut parts).await {
                Ok(cub_req) => {
                    // Insert CubReqImpl as an extension
                    parts.extensions.insert(cub_req);

                    // Reconstruct the request and continue
                    let req = Request::from_parts(parts, body);
                    let mut inner_service = inner;
                    inner_service.call(req).await
                }
                Err(legacy_reply) => {
                    // Convert the legacy reply error into a response and return early
                    match legacy_reply {
                        Ok(response) => Ok(response),
                        Err(err_response) => Ok(err_response.into_response()),
                    }
                }
            }
        })
    }
}

async fn create_cub_req_impl(parts: &mut Parts) -> Result<CubReqImpl, LegacyReply> {
    let path = Route::new(parts.uri.path().to_string()).trim_trailing_slash();

    let host = match host_extract::ExtractedHost::from_headers(&parts.uri, &parts.headers) {
        Some(host) => host,
        None => {
            log::warn!(
                "No host found for request uri {} / host header {:?}",
                parts.uri,
                parts.headers.get(header::HOST)
            );
            return Err((StatusCode::BAD_REQUEST, "No host found in request").into_legacy_reply());
        }
    };
    let domain = host.domain();
    let tenant = match host.resolve_domain() {
        Some(DomainResolution::Tenant(ts)) => ts.clone(),
        Some(DomainResolution::Redirect { tenant, .. }) => tenant.clone(),
        None => {
            log::warn!("No tenant found for domain {domain}");
            let msg = if Environment::default().is_dev() {
                let global_state = global_state();
                let available_tenants = global_state
                    .dynamic
                    .read()
                    .tenants_by_name
                    .values()
                    .map(|ts| {
                        let tc = ts.tc();
                        format!(
                            "<li><a href=\"{}\">{}</a></li>",
                            tc.web_base_url(global_state.web),
                            tc.name
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                format!(
                    r#"
                    <html>
                    <head>
                        <style>
                        body {{
                            font-family: system-ui, -apple-system, sans-serif;
                            max-width: 800px;
                            margin: 2rem auto;
                            line-height: 1.5;
                        }}
                        code {{
                            background: #eee;
                            padding: 0.2em 0.4em;
                            border-radius: 3px;
                        }}
                        </style>
                    </head>
                    <body>
                        <h1>No tenant found for domain <code>{domain}</code></h1>
                        <p>Available tenants:</p>
                        <ul>
                            {available_tenants}</ul>
                    </body>
                    </html>
                    "#
                )
            } else {
                "tenant_not_found".to_string()
            };

            let resp = (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                msg,
            );
            // lol
            return Err(Ok(resp.into_response()));
        }
    };

    // Create a dummy state for the Cookies extractor
    // We need to extract cookies, but the FromRequestParts trait requires a state
    // For our purposes, we can use () as the state since Cookies doesn't use it
    let state = ();
    let public_cookies =
        match <Cookies as FromRequestParts<()>>::from_request_parts(parts, &state).await {
            Ok(cookies) => cookies,
            Err(e) => return Err(e.into_legacy_reply()),
        };

    let mut auth_bundle =
        authbundle_load_from_cookies(&public_cookies.private(&tenant.cookie_key)).await;

    if let Some(query) = parts.uri.query() {
        let params: std::collections::HashMap<String, String> =
            form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect();

        if let Some(api_key) = params.get("api_key") {
            if auth_bundle.is_none() {
                // Try to validate the API key with mom
                let tcli = tenant.tcli();
                match tcli
                    .verify_api_key(&VerifyApiKeyArgs {
                        api_key: UserApiKey::new(api_key.clone()),
                    })
                    .await
                {
                    Ok(response) => {
                        log::info!("Validated API key for {}", response.user_info.name());
                        auth_bundle = Some(AuthBundle {
                            user_info: response.user_info,
                        });
                    }
                    Err(e) => {
                        log::warn!("Failed to verify API key: {e}");
                    }
                }
            }
        }
    }

    let viewer = conflux::Viewer::new(
        tenant.rc().map_err(|e| {
            log::error!("Failed to get tenant rc: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_legacy_reply()
        })?,
        auth_bundle.as_ref().map(|creds| &creds.user_info),
        conflux::AccessOverride::from_raw_query(parts.uri.query().unwrap_or_default()),
    );

    let cub_req = CubReqImpl {
        cookie_key: tenant.cookie_key.clone(),
        public_cookies,
        tenant,
        path,
        auth_bundle,
        viewer,
        parts: parts.clone(),
    };

    Ok(cub_req)
}
