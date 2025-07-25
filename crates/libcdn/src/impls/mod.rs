use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bytesize::ByteSize;
use config_types::{TenantConfig, WebConfig};
use conflux::{Asset, PathMappings, Route};
use content_type::ContentType;
use cub_types::CubReq;
use derivations::DerivationInfo;
use eyre::bail;
use hattip::http::Uri;
use libhttpclient::HttpClient;
use mom_types::{DeriveParams, DeriveResponse};

use hattip::prelude::*;
use hattip::to_herror;
use libwebsock::{Message, WebSocketStream};

pub(crate) async fn serve_asset(rcx: Box<dyn CubReq>, headers: HeaderMap) -> HReply {
    let tenant = rcx.tenant_owned();

    let web = rcx.web();
    let env = web.env;
    let route = rcx.route();
    log::debug!("Serving asset \x1b[1;32m{route}\x1b[0m");

    if env.is_dev() && route.as_str().starts_with("/dist") {
        return proxy_to_vite(rcx).await;
    }

    let lrev = tenant.rev().map_err(to_herror)?;
    let rev = &lrev.rev;
    let asset = rev
        .assets
        .get(route)
        .ok_or_else(|| HError::with_status(StatusCode::NOT_FOUND, "no such asset"))?;

    match asset {
        Asset::Inline {
            content,
            content_type,
        } => {
            log::trace!("Found inline asset route");
            let body = HBody::from(content.clone());
            asset_response_builder(tenant.tc(), web, *content_type)
                .body(body)
                .into_reply()
        }
        Asset::Derivation(derivation) => {
            log::trace!("Found derivation asset route");
            let input = rev.pak.inputs.get(&derivation.input).ok_or_else(|| {
                log::warn!("Input not found for path: {:?}", &derivation.input);
                HError::with_status(StatusCode::NOT_FOUND, "input not found for path")
            })?;
            log::trace!("Found derivation input: {}", input.path);

            let di = DerivationInfo::new(input, derivation);
            let content_type = di.content_type();
            let bytes = derive(rcx.as_ref(), di).await.map_err(to_herror)?;

            // Build base response with common headers
            let mut res = asset_response_builder(tenant.tc(), web, content_type);

            // Handle range requests
            if let Some(range_header) = headers.get(header::RANGE) {
                if let Ok(ranges) = http_range::HttpRange::parse(
                    range_header.to_str().unwrap_or(""),
                    bytes.len() as _,
                ) {
                    // For now just handle the first range
                    let range = &ranges[0];
                    let content_length = range.length;
                    let range_header = format!(
                        "bytes {}-{}/{}",
                        range.start,
                        range.start + content_length - 1,
                        bytes.len()
                    );

                    res = res
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(header::CONTENT_LENGTH, content_length.to_string())
                        .header(header::CONTENT_RANGE, range_header)
                        .header(header::ACCEPT_RANGES, "bytes");

                    let start = range.start as usize;
                    let end = (range.start + content_length) as usize;
                    let body = HBody::from(bytes.slice(start..end));
                    return res.body(body).unwrap().into_reply();
                }
            }

            // Return full response if no range or invalid range
            res.status(StatusCode::OK)
                .header(header::ACCEPT_RANGES, "bytes")
                .body(HBody::from(bytes))
                .into_reply()
        }
        Asset::AcceptBasedRedirect { options } => {
            if options.is_empty() {
                log::error!("No options available for accept-based redirect");
                return Err(HError::with_status(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "No options available for accept-based redirect",
                ));
            }

            let accept = headers.get("Accept").and_then(|h| h.to_str().ok());
            let option: Option<Route> = accept.and_then(|accept| {
                for (ct, route) in options.iter() {
                    // this isn't at all how negotiation works, but it'll work for the accept-based redirects
                    // we have right now — if `image/jxl` is explicitly listed, we can use it. `image/*` does not
                    // actually support `image/jxl`.
                    if accept.contains(ct.as_str()) {
                        log::debug!(
                            "\x1b[36mPicked \x1b[35m{}\x1b[36m for Accept: \x1b[33m{accept}\x1b[0m",
                            ct.as_str()
                        );
                        return Some(route.clone());
                    }
                }
                None
            });

            let route = match &option {
                Some(route) => route.clone(),
                // the last option is the most compatible
                None => options.last().unwrap().1.clone(),
            };

            let redirect_url = route.to_cdn_url_string(tenant.tc(), rcx.web());
            Response::builder()
                .status(StatusCode::TEMPORARY_REDIRECT)
                .header(header::LOCATION, redirect_url.as_str())
                .header(
                    header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    tenant.tc().web_base_url(rcx.web()),
                )
                .body(HBody::empty())
                .into_reply()
        }
    }
}

fn asset_response_builder(
    tc: &TenantConfig,
    web: WebConfig,
    content_type: ContentType,
) -> response::Builder {
    Response::builder()
        .header(header::CONTENT_TYPE, content_type.as_str())
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, tc.web_base_url(web))
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "max-age=31536000")
}

async fn derive(rcx: &dyn CubReq, di: DerivationInfo<'_>) -> eyre::Result<Bytes> {
    let env = rcx.web().env;
    let tenant = rcx.tenant_ref();

    // has the derivation already been made? if so, return it
    let cache_key = di.key(env);
    match tenant.store().get(&cache_key).await {
        Ok(res) => {
            log::debug!("Found derivation in cache: {cache_key:?}");
            return res.bytes().await.map_err(|e| {
                eyre::eyre!(
                    "failed to fetch bytes from upstream for cache key '{}': {}",
                    cache_key,
                    e
                )
            });
        }
        Err(e) => {
            if e.is_not_found() {
                // all good
                log::debug!("cache miss: {cache_key}");
            } else {
                log::warn!("error while fetching from cache ({cache_key}): {e}")
            }
        }
    }

    // kindly ask mom to run the derivation
    let tcli = tenant.tcli();

    let input_key = di.input.key();
    if env.is_dev() {
        // in dev, the input might not be on object storage yet, so... check for it and upload it if needed
        if tenant.store().get(&input_key).await.is_err() {
            log::info!("Uploading input to object storage in development mode: {input_key}");
            let mappings = PathMappings::from_ti(tenant.ti());
            let disk_path = mappings.to_disk_path(&di.input.path)?;
            // TODO: don't buffer the whole file in memory
            let bytes = fs_err::tokio::read(&disk_path).await?;
            tenant.store().put(&input_key, bytes.into()).await?;
        } else {
            log::info!(
                "Input is already in object storage: {}, object_store = {}",
                input_key,
                tenant.store().desc()
            );
        }
    }

    let start = Instant::now();
    let route = di.route();

    let mut tries = 0;
    let mut sleep_ms = 200;
    let max_tries = 20;
    loop {
        tries += 1;
        if tries > max_tries {
            bail!("max retries ({}) exceeded waiting for derivation", tries);
        }

        log::info!("Asking mom to derive (input_key: {input_key}, route: {route})");
        let res = tcli
            .derive(DeriveParams {
                input: di.input.clone(),
                derivation: di.derivation.clone(),
            })
            .await?;
        match res {
            DeriveResponse::Done(donezo) => {
                let written_to = donezo.dest;
                if written_to != cache_key {
                    bail!(
                        "derivation output key ({}) does not match expected key ({})",
                        written_to,
                        cache_key
                    );
                }
                log::info!(
                    "\x1b[36m{} => {}\x1b[0m took \x1b[32m{:?}\x1b[0m (\x1b[34m{}\x1b[0m => \x1b[34m{}\x1b[0m, e.g. \x1b[35m{:.2}x\x1b[0m) \x1b[33m{}\x1b[0m",
                    di.input.path.explode().1,
                    di.derivation.kind,
                    start.elapsed(),
                    ByteSize::b(di.input.size),
                    ByteSize::b(donezo.output_size as u64),
                    donezo.output_size as f64 / di.input.size as f64,
                    route
                );
                break;
            }
            DeriveResponse::AlreadyInProgress(inprog) => {
                log::info!("Derivation {route} is already in progress: {inprog:?}");

                sleep_ms = std::cmp::min(2000, sleep_ms + 100);
                tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
            }
            DeriveResponse::TooManyRequests(_) => {
                log::warn!("Too many requests for derivation {route}");
                sleep_ms = std::cmp::min(5000, sleep_ms * 2);
                tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
            }
        }
    }

    // according to mom, it's now available in the object store, fetch it
    let res = tenant.store().get(&cache_key).await?;
    return res.bytes().await.map_err(|e| {
        eyre::eyre!(
            "failed to fetch bytes from upstream for cache key '{}': {}",
            cache_key,
            e
        )
    });
}

static VITE_HTTP_CLIENT: LazyLock<Arc<dyn HttpClient>> =
    LazyLock::new(|| Arc::from(libhttpclient::load().client()));

async fn proxy_to_vite(rcx: Box<dyn CubReq>) -> HReply {
    let port = rcx.tenant_ref().vite_port().await.map_err(|e| {
        HError::with_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("vite dev server failed to start: {e}"),
        )
    })?;

    let src_uri = rcx.uri().clone();
    let src_headers = rcx.parts().headers.clone();

    rcx.parts();
    let dst_uri = Uri::builder()
        .scheme("http")
        .authority(format!("localhost:{port}"))
        .path_and_query(src_uri.path_and_query().unwrap().clone())
        .build()
        .unwrap();
    log::debug!("Proxying \x1b[32m{src_uri}\x1b[0m => \x1b[33m{dst_uri}\x1b[0m");

    if rcx.has_ws() {
        log::debug!("Has websocket upgrade!!");

        let dst_uri = Uri::builder()
            .scheme("ws")
            .authority(format!("localhost:{port}"))
            .path_and_query(dst_uri.path_and_query().unwrap().clone())
            .build()
            .unwrap();

        let ws_protocol = src_headers
            .get("Sec-WebSocket-Protocol")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut res = rcx
            .on_ws_upgrade(Box::new(|downstream_ws| {
                tokio::spawn(async move {
                    log::trace!("[WS_PROXY] We made it all the way to the upgrade!!! yay!");
                    log::debug!(
                        "[WS_PROXY] Proxying from \x1b[32m{src_uri}\x1b[0m to vite's websocket at \x1b[33m{dst_uri}\x1b[0m"
                    );

                    log::trace!(
                        "[WS_PROXY] Incoming ws request headers: {headers}",
                        headers = src_headers
                            .iter()
                            .map(|(k, v)| format!("{k}: {}", v.to_str().unwrap_or("<binary>")))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );

                    let upstream = match libwebsock::load()
                        .websocket_connect(dst_uri.clone(), src_headers.clone())
                        .await
                    {
                        Ok(conn) => conn,
                        Err(e) => {
                            log::error!("[WS_PROXY] Failed to connect to upstream websocket: {e}");
                            return;
                        }
                    };

                    log::trace!("[WS_PROXY] Connected!");

                    let res: eyre::Result<()> = do_ws_proxy( upstream, downstream_ws).await;
                    match res {
                        Ok(_) => log::debug!("WebSocket connection closed gracefully"),
                        Err(e) => log::error!("Error in websocket connection: {e}"),
                    }
                });
            }))
            .await?;
        // Pass along the Sec-WebSocket-Protocol header if present
        if let Some(protocol) = ws_protocol {
            log::trace!("Adding Sec-WebSocket-Protocol header: {protocol}");
            res.headers_mut()
                .insert("Sec-WebSocket-Protocol", protocol.parse().unwrap());
        } else {
            log::trace!("No Sec-WebSocket-Protocol header present");
        }

        Ok(res)
    } else {
        let client = VITE_HTTP_CLIENT.clone();
        let response = client.get(dst_uri).send().await.map_err(|e| {
            HError::with_status(
                StatusCode::BAD_GATEWAY,
                format!("failed to proxy to vite dev server: {e}"),
            )
        })?;
        let status = response.status();
        let headers = response.headers_only_string_safe().clone();
        let bytes = response.bytes().await.map_err(|e| {
            HError::with_status(
                StatusCode::BAD_GATEWAY,
                format!("failed to read response from vite dev server: {e}"),
            )
        })?;

        let mut builder = Response::builder().status(status);
        for (name, value) in headers.iter() {
            builder = builder.header(name, value);
        }
        let response = builder.body(HBody::from(bytes)).map_err(|e| {
            HError::with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to build response: {e}"),
            )
        })?;
        response.into_reply()
    }
}

async fn do_ws_proxy(
    mut upstream: Box<dyn WebSocketStream>,
    mut downstream: Box<dyn WebSocketStream>,
) -> eyre::Result<()> {
    enum Event {
        FromUpstream(Option<Result<Message, libhttpclient::Error>>),
        FromDownstream(Option<Result<Message, libhttpclient::Error>>),
    }

    loop {
        log::trace!("[WS_PROXY] Waiting for message from either peer...");
        let ev = tokio::select! {
            // Handle messages from upstream (vite) to downstream (client)
            upstream_msg = upstream.receive() => Event::FromUpstream(upstream_msg),

            // Handle messages from downstream (client) to upstream (vite)
            downstream_msg = downstream.receive() => Event::FromDownstream(downstream_msg)
        };

        match ev {
            Event::FromUpstream(Some(Ok(msg))) => {
                log::trace!("[WS_PROXY] Upstream → Downstream: forwarding message: {msg:?}");
                if let Err(e) = downstream.send(msg).await {
                    log::error!("[WS_PROXY] Error forwarding message to downstream: {e}");
                    break;
                }
                log::trace!("[WS_PROXY] forwarded to downstream!");
            }
            Event::FromDownstream(Some(Ok(msg))) => {
                log::trace!("[WS_PROXY] Downstream → Upstream: forwarding message: {msg:?}");
                if let Err(e) = upstream.send(msg).await {
                    log::error!("[WS_PROXY] Error forwarding message to upstream: {e}");
                    break;
                }
                log::trace!("[WS_PROXY] forwarded to upstream!");
            }
            Event::FromUpstream(None) => {
                log::trace!("[WS_PROXY] Received None from upstream, closing connection");
                break;
            }
            Event::FromDownstream(None) => {
                log::trace!("[WS_PROXY] Received None from downstream, closing connection");
                break;
            }
            Event::FromUpstream(Some(Err(e))) => {
                log::error!("[WS_PROXY] Error receiving message from upstream: {e}");
                break;
            }
            Event::FromDownstream(Some(Err(e))) => {
                log::error!("[WS_PROXY] Error receiving message from downstream: {e}");
                break;
            }
        }
    }
    log::trace!("[WS_PROXY] Stopping websocket connection");
    Ok(())
}
