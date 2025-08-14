use std::collections::HashSet;

use crate::Result;
use axum::extract::ws;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Router, extract::FromRequestParts};
use config_types::{MomApiKeyRef, TenantDomain, is_development};
use futures_util::SinkExt;
use log::{error, info, warn};
use tenant_extractor::TenantExtractor;
use tokio::signal::unix::SignalKind;

use crate::impls::{MomGlobalState, global_state};
use mom_types::{GoodMorning, MomEvent, TenantInitialState};

mod tenant;
mod tenant_extractor;
mod wip;

/// Inserted in the request context to indicate that the used API key is
#[derive(Clone, Debug)]
pub enum KeyPermissions {
    /// The skeleton key, which is not scoped to any tenant.
    Skeleton,
    /// Scoped to a specific tenant set.
    Tenants(HashSet<TenantDomain>),
}

impl KeyPermissions {
    pub fn has_access_to(&self, td: &TenantDomain) -> bool {
        match self {
            KeyPermissions::Skeleton => true,
            KeyPermissions::Tenants(tenants) => tenants.contains(td),
        }
    }
}

pub(super) async fn serve(listener: tokio::net::TcpListener) -> Result<()> {
    let app = Router::new()
        .nest(
            "/tenant/{tenant_name}",
            tenant::tenant_routes().layer(
                axum::middleware::from_fn(
                    |req: axum::http::Request<axum::body::Body>, next: axum::middleware::Next| async move {
                        let (mut parts, body) = req.into_parts();
                        let extractor = match TenantExtractor::from_request_parts(&mut parts, &()).await {
                            Ok(ts) => ts,
                            Err(reply) => {
                                return reply.into_response()
                            }
                        };

                        match parts.extensions.get::<KeyPermissions>() {
                            Some(kp) if kp.has_access_to(&extractor.0.ti.tc.name) => {
                                parts.extensions.insert(extractor);
                                let req = axum::http::Request::from_parts(parts, body);
                                next.run(req).await
                            },
                            _ => axum::http::StatusCode::UNAUTHORIZED.into_response(),
                        }

                    }
                )
            ),
        )
        .route("/events", get(get_events))
        .layer(axum::middleware::from_fn(
            move |mut req: axum::http::Request<axum::body::Body>, next: axum::middleware::Next| {
                async move {
                    let mom_secrets = &global_state().config.secrets;
                    let auth_header = req.headers().get(axum::http::header::AUTHORIZATION);
                    let mut key_permissions = None;

                    if let Some(key) = auth_header.and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer ")).map(MomApiKeyRef::from_str) {
                        if key == mom_secrets.readonly_api_key {
                            // master key, not scoped
                            key_permissions = Some(KeyPermissions::Skeleton);
                        } else if let Some(scoped) = mom_secrets.scoped_api_keys.get(key) {
                            key_permissions = Some(KeyPermissions::Tenants(
                                // todo: add HashSet stuff to facet, for now, we work around.
                                scoped.tenants.iter().cloned().collect()
                            ));
                        }
                    }

                    match key_permissions {
                        Some(kp) => {
                            req.extensions_mut().insert(kp);
                            next.run(req).await
                        },
                        None => {
                            axum::http::StatusCode::UNAUTHORIZED.into_response()
                        }
                    }
                }
            },
        ))
        .route("/wip/{tenant_name}", get(wip::serve_wip))
        .route("/health", get(|| async { "OK" }))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .layer(
            axum::middleware::from_fn(
                |req: axum::http::Request<axum::body::Body>, next: axum::middleware::Next| async move {
                    let path = req.uri().path().to_owned();
                    let query = req.uri().query().map(|q| q.to_owned());
                    let method = req.method().clone();
                    let start = std::time::Instant::now();
                    let response = next.run(req).await;
                    let duration = start.elapsed();
                    let status = response.status();
                    if !path.starts_with("/health") {
                        if let Some(q) = query {
                            log::info!("\x1b[36m{}\x1b[0m \x1b[33m{}\x1b[0m\x1b[90m?\x1b[0m\x1b[32m{}\x1b[0m -> \x1b[35m{}\x1b[0m (took {:?})", method, path, q, status.as_u16(), duration);
                        } else {
                            log::info!("\x1b[36m{}\x1b[0m \x1b[33m{}\x1b[0m -> \x1b[35m{}\x1b[0m (took {:?})", method, path, status.as_u16(), duration);
                        }
                    }
                    response
                }
            )
        );

    let quit_sig = async {
        let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();
        // await either signal
        tokio::select! {
            _ = sigint.recv() => {
                warn!("Received SIGINT");
            },
            _ = sigterm.recv() => {
                warn!("Received SIGTERM");
            },
        }
        if is_development() {
            warn!("Exiting immediately");
            std::process::exit(0);
        }

        warn!("Initiating graceful shutdown");

        tokio::spawn(async move {
            // if we receive a second signal, exit ungracefully immediately
            sigint.recv().await;
            error!("Received second signal, exiting ungracefully");
            std::process::exit(1);
        });
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(quit_sig)
        .await?;
    Ok(())
}

async fn get_events(ws: axum::extract::WebSocketUpgrade) -> impl axum::response::IntoResponse {
    info!("got /events request");
    ws.on_failed_upgrade(|err| {
        warn!("Failed to upgrade to WebSocket: {err}");
    })
    .on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: ws::WebSocket) {
    info!("connection upgraded to websocket!");

    let gs = global_state();
    let mut rx = gs.bx_event.subscribe();

    // Send good morning message!
    let mut gm = GoodMorning {
        initial_states: Default::default(),
    };

    for (tn, ts) in gs.tenants.iter() {
        let revision = ts.pak.lock().clone();
        let users = ts.users.lock().clone();
        log::info!(
            "in good morning, for tenant {}, sending {} users (-1 means None)",
            tn,
            users.as_ref().map(|s| s.users.len() as isize).unwrap_or(-1)
        );

        // Create a TenantConfig with derived cookie sauce
        let mut tc = ts.ti.tc.clone();
        if let Some(ref mut secrets) = tc.secrets {
            if secrets.cookie_sauce.is_none() {
                let global_cookie_sauce = &gs.config.secrets.cookie_sauce;
                let derived_sauce = mom_types::derive_cookie_sauce(global_cookie_sauce, tn);
                secrets.cookie_sauce = Some(derived_sauce);
            }
        }

        gm.initial_states.insert(
            tn.clone(),
            TenantInitialState {
                pak: revision,
                users,
                tc,
                base_dir: if is_development() {
                    // in dev, let mom and cub share a base directory
                    Some(ts.ti.base_dir.clone())
                } else {
                    None
                },
            },
        );
    }

    let msg = MomGlobalState::event_to_message(MomEvent::GoodMorning(gm));
    if let Err(e) = socket.send(msg).await {
        log::error!("Failed to send good morning: {e}");
        return;
    }

    if let Err(e) = socket.flush().await {
        log::error!("Failed to flush WebSocket message: {e}");
        return;
    }

    log::info!("Starting WebSocket message loop");
    loop {
        tokio::select! {
            Ok(json_payload) = rx.recv() => {
                let msg = ws::Message::text(json_payload);
                if let Err(e) = socket.send(msg).await {
                    log::error!("Failed to send WebSocket message: {e}");
                    break;
                }
                if let Err(e) = socket.flush().await {
                    log::error!("Failed to flush WebSocket message: {e}");
                    break;
                }
            }
            Some(result) = socket.recv() => {
                match result {
                    Ok(_) => {
                        // Ignore received messages
                        log::debug!("Received message from WebSocket (ignored)");
                    }
                    Err(e) => {
                        log::error!("Error receiving WebSocket message: {e}");
                        break;
                    }
                }
            }
            else => break,
        }
    }
    log::info!("WebSocket message loop ended");
}
