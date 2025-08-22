use std::task::{Context, Poll};

use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode, header, request::Parts},
    response::IntoResponse as _,
};
use futures_core::future::BoxFuture;
use opentelemetry::{
    KeyValue,
    trace::{FutureExt, TraceContextExt, Tracer},
};
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

                    let tracer = opentelemetry::global::tracer("http");
                    let method = &parts.method;
                    let uri = &parts.uri;

                    let mut span = tracer.span_builder(format!("{method} {uri}"));
                    let mut attributes: Vec<KeyValue> = vec![];
                    // HTTP request method (required)
                    attributes.push(opentelemetry::KeyValue::new(
                        "http.request.method",
                        parts.method.to_string(),
                    ));

                    // URL path (required for server spans)
                    attributes.push(opentelemetry::KeyValue::new(
                        "url.path",
                        parts.uri.path().to_string(),
                    ));

                    // URL scheme (required for server spans)
                    let scheme = parts.uri.scheme_str().unwrap_or("http").to_string();
                    attributes.push(opentelemetry::KeyValue::new("url.scheme", scheme.clone()));

                    // URL query (conditionally required if present)
                    if let Some(query) = parts.uri.query() {
                        attributes
                            .push(opentelemetry::KeyValue::new("url.query", query.to_string()));
                    }

                    // Client address from various headers (recommended)
                    if let Some(forwarded_for) = parts.headers.get("x-forwarded-for") {
                        if let Ok(forwarded_str) = forwarded_for.to_str() {
                            // Take the first IP from the comma-separated list
                            let client_ip = forwarded_str
                                .split(',')
                                .next()
                                .unwrap_or(forwarded_str)
                                .trim();
                            attributes.push(opentelemetry::KeyValue::new(
                                "client.address",
                                client_ip.to_string(),
                            ));
                        }
                    } else if let Some(real_ip) = parts.headers.get("x-real-ip") {
                        if let Ok(ip_str) = real_ip.to_str() {
                            attributes.push(opentelemetry::KeyValue::new(
                                "client.address",
                                ip_str.to_string(),
                            ));
                        }
                    }

                    // User agent (recommended)
                    if let Some(user_agent) = parts.headers.get("user-agent") {
                        if let Ok(ua_str) = user_agent.to_str() {
                            attributes.push(opentelemetry::KeyValue::new(
                                "user_agent.original",
                                ua_str.to_string(),
                            ));
                        }
                    }

                    // Server address and port from Host header (recommended)
                    if let Some(host_header) = parts.headers.get("host") {
                        if let Ok(host_str) = host_header.to_str() {
                            if let Some((server_addr, server_port)) = host_str.split_once(':') {
                                attributes.push(opentelemetry::KeyValue::new(
                                    "server.address",
                                    server_addr.to_string(),
                                ));
                                if let Ok(port) = server_port.parse::<i64>() {
                                    attributes
                                        .push(opentelemetry::KeyValue::new("server.port", port));
                                }
                            } else {
                                attributes.push(opentelemetry::KeyValue::new(
                                    "server.address",
                                    host_str.to_string(),
                                ));
                                // Default ports based on scheme
                                let default_port = if scheme == "https" { 443 } else { 80 };
                                attributes.push(opentelemetry::KeyValue::new(
                                    "server.port",
                                    default_port,
                                ));
                            }
                        }
                    }

                    // Network protocol name and version (conditionally required)
                    attributes.push(opentelemetry::KeyValue::new(
                        "network.protocol.name",
                        "http",
                    ));
                    // HTTP version detection based on the request
                    let protocol_version = match parts.version {
                        axum::http::Version::HTTP_09 => "0.9",
                        axum::http::Version::HTTP_10 => "1.0",
                        axum::http::Version::HTTP_11 => "1.1",
                        axum::http::Version::HTTP_2 => "2",
                        axum::http::Version::HTTP_3 => "3",
                        _ => "1.1", // default fallback
                    };
                    attributes.push(opentelemetry::KeyValue::new(
                        "network.protocol.version",
                        protocol_version,
                    ));
                    span = span.with_attributes(attributes);
                    let span = span.start(&tracer);
                    let otel_cx = opentelemetry::context::Context::current_with_span(span);

                    // Reconstruct the request and continue
                    let req = Request::from_parts(parts, body);
                    let mut inner_service = inner;
                    let res = inner_service.call(req).with_context(otel_cx.clone()).await;
                    match &res {
                        Ok(http_res) => {
                            let span = otel_cx.span();

                            // Set the HTTP response status code
                            span.set_attribute(opentelemetry::KeyValue::new(
                                "http.response.status_code",
                                http_res.status().as_u16() as i64,
                            ));

                            // Set HTTP response headers (Opt-In)
                            for (name, value) in http_res.headers() {
                                if let Ok(value_str) = value.to_str() {
                                    let header_key = format!(
                                        "http.response.header.{}",
                                        name.as_str().to_lowercase()
                                    );
                                    span.set_attribute(opentelemetry::KeyValue::new(
                                        header_key,
                                        value_str.to_string(),
                                    ));
                                }
                            }

                            // Set response body size if Content-Length header is present (Opt-In)
                            if let Some(content_length) = http_res.headers().get("content-length") {
                                if let Ok(length_str) = content_length.to_str() {
                                    if let Ok(length) = length_str.parse::<i64>() {
                                        span.set_attribute(opentelemetry::KeyValue::new(
                                            "http.response.body.size",
                                            length,
                                        ));
                                    }
                                }
                            }
                        }
                        Err(_e) => {
                            // muffin
                        }
                    }
                    res
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
