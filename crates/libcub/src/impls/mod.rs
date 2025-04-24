use conflux::PathMappings;
use cub_types::{CubRevisionState, CubTenant as _};
use global_state::global_state;
use graceful_shutdown::setup_graceful_shutdown;
use hattip::{HBody, HError, HReply};
use libc as _;

use axum::{Router, body::Body, extract::DefaultBodyLimit};
use config_types::{
    CubConfig, Environment, MOM_DEV_API_KEY, MomApiKey, TenantDomain, TenantInfo, WebConfig,
    is_development, is_production,
};
use futures_core::future::BoxFuture;
use itertools::Itertools;
use layers::{
    compression::CompressionLayer, domain_redirect::DomainRedirectLayer,
    strip_slash_if_404::StripSlashIf404Layer,
};
use libmomclient::{MomClient, MomClientConfig, MomEventListener};
use librevision::{RevisionKind, RevisionSpec};
use mom_event_handler::spawn_mom_event_handler;
use mom_types::{MomEvent, Sponsors};
use node_metadata::{NodeMetadata, load_node_metadata};
use parking_lot::RwLock;
use reply::{LegacyHttpError, LegacyReply};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
};
use tower::{ServiceBuilder, steer::Steer};
use tower_cookies::CookieManagerLayer;
use tracing::{info, warn};
use types::CubDynamicState;

pub mod access_control;
pub mod cdn;
pub mod credentials;
pub mod cub_req;
pub mod global_state;
mod graceful_shutdown;
pub mod host_extract;
pub mod layers;
mod node_metadata;
pub mod path_metadata;
pub mod reply;
pub mod types;
pub mod vite;
pub mod web;

use crate::OpenBehavior;

use self::types::{CubGlobalState, CubTenantImpl, DomainResolution};

pub(crate) async fn serve(
    cc: CubConfig,
    ln: TcpListener,
    open_behavior: OpenBehavior,
) -> eyre::Result<()> {
    let metadata = load_node_metadata().await?;

    let web = WebConfig {
        env: Environment::default(),
        port: cc.address.port(),
    };

    let mom_client_config = MomClientConfig {
        base_url: cc.mom_base_url.clone(),
        api_key: Some(cc.mom_api_key.clone()),
    };
    let (mom_client, mut mev_rx) = setup_mom_client(mom_client_config).await?;

    let (tenant_infos, mut revs_per_ts, mut sponsors_per_ts) =
        process_mom_good_morning(&cc, &mut mev_rx, web).await?;

    let deploy_mom_client = if web.env.is_prod() {
        mom_client.clone()
    } else {
        {
            let base_url = "https://mom.bearcove.cloud".to_string();
            let api_key: MomApiKey = match std::env::var("MOM_API_KEY") {
                Ok(key) => key.into(),
                Err(_) => MOM_DEV_API_KEY.to_owned(),
            };
            let client = libmomclient::load()
                .client(MomClientConfig {
                    base_url,
                    api_key: Some(api_key),
                })
                .await?;
            Arc::from(client)
        }
    };

    let gs = build_global_state(
        cc.clone(),
        web,
        mom_client,
        deploy_mom_client,
        &tenant_infos,
        &mut revs_per_ts,
        &mut sponsors_per_ts,
    )
    .await?;
    global_state::set_global_state(Box::leak(Box::new(gs)))
        .unwrap_or_else(|_| panic!("GLOBAL_STATE must be set only once"));

    if is_production() {
        // We're doing this late because we need the global state to be set.
        spawn_mom_event_handler(mev_rx, web);
    } else {
        start_watching_revisions().await?;
    }

    let app = setup_app_routes(&metadata).await?;
    let quit_sig = setup_graceful_shutdown();
    log_tenant_urls(&cc);

    if matches!(open_behavior, OpenBehavior::OpenOnStart) {
        let web = cc.web_config();
        if let Some(ti) = tenant_infos.values().next() {
            let url = ti.tc.web_base_url(web);
            if let Err(e) = open::that(url) {
                warn!("Failed to open browser: {e}");
            }
        }
    }

    axum::serve(
        ln,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(quit_sig)
    .await
    .map_err(|e| eyre::eyre!("Failed to serve: {}", e))?;

    Ok(())
}

struct MomEventRelay {
    mev_tx: mpsc::Sender<MomEvent<'static>>,
}

impl MomEventListener for MomEventRelay {
    fn on_event<'fut>(&'fut self, event: MomEvent<'static>) -> BoxFuture<'fut, ()> {
        Box::pin(async move {
            self.mev_tx.send(event).await.unwrap();
        })
    }
}

async fn setup_mom_client(
    mcc: MomClientConfig,
) -> eyre::Result<(Arc<dyn MomClient>, mpsc::Receiver<MomEvent<'static>>)> {
    let mod_momclient = libmomclient::load();
    let mom_client = mod_momclient
        .client(mcc.clone())
        .await
        .map_err(|e| eyre::eyre!("Failed to create mom client: {}", e))?;

    let (mev_tx, mev_rx) = tokio::sync::mpsc::channel::<MomEvent<'static>>(2);

    mod_momclient
        .subscribe_to_mom_events(Box::new(MomEventRelay { mev_tx }), mcc)
        .await
        .map_err(|e| eyre::eyre!("Failed to subscribe to mom events: {}", e))?;

    Ok((Arc::from(mom_client), mev_rx))
}

async fn process_mom_good_morning(
    cc: &CubConfig,
    mev_rx: &mut mpsc::Receiver<MomEvent<'static>>,
    web: WebConfig,
) -> eyre::Result<(
    HashMap<TenantDomain, Arc<TenantInfo>>,
    HashMap<TenantDomain, CubRevisionState>,
    HashMap<TenantDomain, Sponsors<'static>>,
)> {
    let mod_revision = librevision::load();
    let mut revs_per_ts: HashMap<TenantDomain, CubRevisionState> = Default::default();
    let mut sponsors_per_ts: HashMap<TenantDomain, Sponsors<'static>> = Default::default();

    info!(
        "Waiting for mom's good morning message to initialize tenants and start serving content..."
    );
    let mom_event = mev_rx.recv().await;

    let gm = match mom_event {
        Some(MomEvent::GoodMorning(gm)) => gm,
        Some(ev) => {
            panic!(
                "Expected to receive good morning, but received unexpected event: {:?}",
                ev
            );
        }
        None => {
            panic!(
                "Expected to receive a good morning from mom, but none was received, and we're in production, so, there."
            );
        }
    };

    let mut tenant_infos: HashMap<TenantDomain, Arc<TenantInfo>> = Default::default();

    for (tn, tis) in gm.initial_states {
        let ti = Arc::new(TenantInfo {
            base_dir: if is_development() {
                tis.base_dir.expect("mom should've given us the base dir")
            } else {
                cc.tenant_data_dir
                    .as_ref()
                    .expect("tenant data dir should be set")
                    .join(tn.as_str())
            },
            tc: tis.tc,
        });

        if let Some(sponsors) = tis.sponsors {
            sponsors_per_ts.insert(tn.clone(), sponsors);
        }
        let mappings = PathMappings::from_ti(&ti);

        let rs = 'load: {
            if let Some(pak) = tis.pak {
                match mod_revision
                    .load_pak(pak, ti.clone(), None, mappings, web)
                    .await
                {
                    Ok(indexed_rv) => CubRevisionState {
                        rev: Some(indexed_rv),
                        err: None,
                    },
                    Err(e) => CubRevisionState {
                        rev: None,
                        err: Some(conflux::RevisionError(format!(
                            "failed to load pak from mom's good morning: {e}"
                        ))),
                    },
                }
            } else {
                if web.env.is_dev() {
                    eprintln!("No revision in good morning, let's make one");
                    break 'load match mod_revision
                        .make_revision(
                            ti.clone(),
                            RevisionSpec {
                                kind: RevisionKind::FromScratch,
                                mappings,
                            },
                            web,
                        )
                        .await
                    {
                        Ok(indexed_rv) => CubRevisionState {
                            rev: Some(indexed_rv),
                            err: None,
                        },
                        Err(e) => CubRevisionState {
                            rev: None,
                            err: Some(conflux::RevisionError(format!(
                                "failed to make revision from scratch: {e}"
                            ))),
                        },
                    };
                }

                CubRevisionState {
                    rev: None,
                    err: Some(conflux::RevisionError(format!(
                        "No revision in good morning for tenant {tn}"
                    ))),
                }
            }
        };
        revs_per_ts.insert(tn.clone(), rs);
        tenant_infos.insert(tn, ti);
    }

    Ok((tenant_infos, revs_per_ts, sponsors_per_ts))
}

/// This function builds the global state for the application, which includes initializing
/// tenants, setting up domain resolutions, and preparing the necessary components for each
/// tenant. It's crucial because it:
/// 1. Creates the central data structure (CubGlobalState) that holds all tenant information
/// 2. Sets up object stores, cookie keys, and revision states for each tenant
/// 3. Configures domain resolutions for web and CDN domains
/// 4. Initializes Vite for development environments
/// 5. Prepares the application to handle requests for multiple tenants efficiently
async fn build_global_state(
    config: CubConfig,
    web: WebConfig,
    mom_client: Arc<dyn MomClient>,
    mom_deploy_client: Arc<dyn MomClient>,
    tenant_infos: &HashMap<TenantDomain, Arc<TenantInfo>>,
    revs_per_ts: &mut HashMap<TenantDomain, CubRevisionState>,
    sponsors_per_ts: &mut HashMap<TenantDomain, Sponsors<'static>>,
) -> eyre::Result<CubGlobalState> {
    let mut gs = CubGlobalState {
        config,
        web,
        mom_client,
        mom_deploy_client,
        dynamic: Arc::new(RwLock::new(CubDynamicState {
            tenants_by_name: Default::default(),
            domain_resolution: Default::default(),
        })),
    };

    for (tn, ti) in tenant_infos {
        let (bx_rev, _) = broadcast::channel(128);
        let object_store = derivations::objectstore_for_tenant(ti, Environment::default())
            .await
            .map_err(|e| eyre::eyre!("Failed to get object store: {}", e))?;
        let cookie_sauce = ti.tc.cookie_sauce();
        assert!(
            !cookie_sauce.is_empty(),
            "[{tn}] cookie sauce cannot be empty"
        );
        let sauce_repetitions = (32 / cookie_sauce.len()) + 1;
        let cookie_master_key = cookie_sauce.into_bytes().repeat(sauce_repetitions);
        let cookie_key = tower_cookies::Key::derive_from(&cookie_master_key);

        let rs = revs_per_ts.remove(tn).unwrap().clone();
        let sponsors = sponsors_per_ts.remove(tn).unwrap_or_else(|| Sponsors {
            sponsors: Default::default(),
        });
        let ts = CubTenantImpl {
            ti: ti.clone(),
            rev_state: RwLock::new(rs),
            bx_rev,
            store: object_store,
            cookie_key: Box::leak(Box::new(cookie_key)),
            sponsors: RwLock::new(Arc::new(sponsors)),
            vite_port: Default::default(),
        };
        let ts = Arc::new(ts);
        gs.dynamic
            .write()
            .tenants_by_name
            .insert(ts.ti.tc.name.clone(), ts.clone());

        setup_domain_resolution(&mut gs, &ts, web);
    }

    Ok(gs)
}

fn setup_domain_resolution(gs: &mut CubGlobalState, ts: &Arc<CubTenantImpl>, web: WebConfig) {
    let web_domain = ts.ti.tc.web_domain(web.env).to_owned();
    let cdn_domain = ts.ti.tc.cdn_domain(web.env);

    {
        let mut dynamic = gs.dynamic.write();

        dynamic
            .domain_resolution
            .insert(web_domain.clone(), DomainResolution::Tenant(ts.clone()));
        dynamic
            .domain_resolution
            .insert(cdn_domain.clone(), DomainResolution::Tenant(ts.clone()));

        for alias in &ts.tc().domain_aliases {
            dynamic.domain_resolution.insert(
                alias.clone(),
                DomainResolution::Redirect {
                    target_domain: web_domain.clone(),
                    tenant: ts.clone(),
                },
            );

            let cdn_alias = TenantDomain::new(format!("cdn.{}", alias));
            dynamic.domain_resolution.insert(
                cdn_alias,
                DomainResolution::Redirect {
                    target_domain: cdn_domain.clone(),
                    tenant: ts.clone(),
                },
            );
        }
    }
}

mod mom_event_handler;

async fn start_watching_revisions() -> eyre::Result<()> {
    let gs = global_state();
    let tenant_arcs = {
        let tenants = gs
            .dynamic
            .read()
            .tenants_by_name
            .values()
            .cloned()
            .collect::<Vec<_>>();
        tenants
            .into_iter()
            .unique_by(|ts| ts.ti.tc.name.clone())
            .collect::<Vec<_>>()
    };
    for ts in tenant_arcs {
        librevision::load()
            .start_watching(ts.clone(), gs.web)
            .await?;
    }
    Ok(())
}

async fn setup_app_routes(metadata: &NodeMetadata<'static>) -> eyre::Result<Router> {
    let pod_name = std::env::var("POD_NAME").ok();
    let node_name = std::env::var("NODE_NAME").ok();

    let source_value = format!(
        "{}.{}.{}",
        metadata.region,
        node_name.as_deref().unwrap_or_default(),
        pod_name.as_deref().unwrap_or_default(),
    );

    let source_layer = layers::set_response_header::SetResponseHeaderLayer::overriding(
        "x-source".try_into().unwrap(),
        http::HeaderValue::try_from(source_value).unwrap(),
    );

    let common_layers = ServiceBuilder::new()
        .layer(CookieManagerLayer::new())
        .layer(source_layer.clone())
        .layer(CompressionLayer::default())
        .layer(StripSlashIf404Layer)
        .layer(DomainRedirectLayer)
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
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
                    if !(path.starts_with("/health")  || (path.starts_with("/dist") && is_development())) {
                        if let Some(q) = query {
                            tracing::info!("\x1b[36m{}\x1b[0m \x1b[33m{}\x1b[0m\x1b[90m?\x1b[0m\x1b[32m{}\x1b[0m -> \x1b[35m{}\x1b[0m (took {:?})", method, path, q, status.as_u16(), duration);
                        } else {
                            tracing::info!("\x1b[36m{}\x1b[0m \x1b[33m{}\x1b[0m -> \x1b[35m{}\x1b[0m (took {:?})", method, path, status.as_u16(), duration);
                        }
                    }
                    response
                }
            )
        );

    let web_routes = web::web_routes().layer(common_layers.clone());
    let cdn_routes = cdn::routes().layer(common_layers.clone());

    let app = {
        let mut services: Vec<Router> = vec![];

        let web_index = services.len();
        services.push(web_routes);

        let cdn_index = services.len();
        services.push(cdn_routes);

        Steer::new(
            services,
            move |req: &axum::http::Request<axum::body::Body>, _services: &[_]| {
                if let Some(domain) =
                    host_extract::ExtractedHost::from_headers(req.uri(), req.headers())
                        .map(|h| h.domain().to_owned())
                {
                    if domain.starts_with("cdn.") {
                        return cdn_index;
                    }
                }
                web_index
            },
        )
    };

    // nasty typing hack so we don't have to name the return type
    Ok(Router::new().fallback_service(app))
}

fn log_tenant_urls(config: &CubConfig) {
    let web = config.web_config();
    for tenant in global_state().dynamic.read().tenants_by_name.values() {
        info!(
            "🦊 Visit the site at \x1b[34m{}\x1b[0m",
            tenant.tc().web_base_url(web)
        );
    }
}

pub fn h_to_axum(hrep: HReply) -> LegacyReply {
    hrep.map(|res| {
        res.map(|body| match body {
            HBody::Empty => Body::empty(),
            HBody::String(s) => Body::from(s),
            HBody::VecU8(bytes) => Body::from(bytes),
            HBody::Bytes(bytes) => Body::from(bytes),
        })
    })
    .map_err(|err| match err {
        HError::WithStatus { status_code, msg } => LegacyHttpError::WithStatus { status_code, msg },
        HError::Internal { err } => LegacyHttpError::Internal { err },
    })
}
