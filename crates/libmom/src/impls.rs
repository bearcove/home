use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, OnceLock},
    time::Duration,
};

use axum::extract::ws;
use config_types::{MomConfig, RevisionConfig, TenantDomain, TenantInfo, WebConfig};
use conflux::{Pak, RevisionId};
use inflight::InflightSlots;
use itertools::Itertools;
use libhttpclient::HttpClient;
use libobjectstore::ObjectStore;
use log::{debug, error, info};
use mom_types::AllUsers;
use objectstore_types::ObjectStoreKey;
use owo_colors::OwoColorize;
use parking_lot::Mutex;
use tokio::sync::broadcast;

use crate::impls::db::mom_db_pool;
use mom_types::{
    DeriveJobInfo, DeriveParams, MomEvent, MomServeArgs, TenantEvent, TenantEventPayload,
    TranscodeJobInfo, TranscodeParams,
};

mod db;
mod deriver;
mod endpoints;
mod ffmpeg;
mod ffmpeg_stream;
mod site;
mod users;

pub(crate) struct MomGlobalState {
    /// shared HTTP client
    pub(crate) client: Arc<dyn HttpClient>,

    /// mom events, already serialized as JSON, for efficient broadcast
    pub(crate) bx_event: broadcast::Sender<String>,

    /// tenants
    pub(crate) tenants: HashMap<TenantDomain, Arc<MomTenantState>>,

    /// config
    pub(crate) config: Arc<MomConfig>,

    /// web config (mostly just port)
    pub(crate) web: WebConfig,
}

pub(crate) struct MomTenantState {
    pub(crate) pool: Pool,

    pub(crate) users_inflight: InflightSlots<(), AllUsers>,
    pub(crate) users: Arc<Mutex<Option<AllUsers>>>,

    pub(crate) pak: Arc<Mutex<Option<Pak>>>,

    pub(crate) object_store: Arc<dyn ObjectStore>,

    pub(crate) transcode_jobs: Mutex<HashMap<TranscodeParams, TranscodeJobInfo>>,
    pub(crate) derive_jobs: Mutex<HashMap<DeriveParams, DeriveJobInfo>>,

    pub(crate) ti: Arc<TenantInfo>,
}

impl MomTenantState {
    /// Returns a clone of the tenant's current revision config.
    /// This locks the pak
    fn rc(&self) -> eyre::Result<RevisionConfig> {
        let pak_guard = self.pak.lock();
        Ok(pak_guard
            .as_ref()
            .ok_or_else(|| eyre::eyre!("no pak"))?
            .rc
            .clone())
    }
}

pub(crate) static GLOBAL_STATE: OnceLock<&'static MomGlobalState> = OnceLock::new();

#[inline]
pub(crate) fn global_state() -> &'static MomGlobalState {
    GLOBAL_STATE.get().unwrap()
}

impl MomGlobalState {
    pub(crate) fn event_to_message(event: MomEvent) -> ws::Message {
        let json_string = facet_json::to_string(&event);
        ws::Message::text(json_string)
    }

    pub(crate) fn broadcast_event(&self, event: MomEvent) -> eyre::Result<()> {
        let ev_debug = format!("{event:?}");
        let event = facet_json::to_string(&event);
        match self.bx_event.send(event) {
            Ok(n) => log::info!("Broadcast to {n} subscribers: {ev_debug}"),
            Err(_) => log::info!("No subscribers for event: {ev_debug}"),
        }

        Ok(())
    }
}

impl MomTenantState {
    pub(crate) fn broadcast_event(&self, payload: TenantEventPayload) -> eyre::Result<()> {
        global_state().broadcast_event(MomEvent::TenantEvent(TenantEvent {
            tenant_name: self.ti.tc.name.clone(),
            payload,
        }))
    }
}

pub(crate) type SqlitePool = r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>;

#[derive(Clone)]
pub(crate) struct Pool(pub(crate) SqlitePool);

impl Deref for Pool {
    type Target = SqlitePool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(crate) async fn load_revision_from_db(ts: &MomTenantState) -> eyre::Result<Option<Pak>> {
    info!("Loading latest revision from database");
    let (id, object_key) = {
        let conn = ts.pool.get()?;
        let mut stmt = conn.prepare(
            "
                SELECT id, object_key
                FROM revisions
                ORDER BY uploaded_at DESC
                LIMIT 1
                ",
        )?;
        let res: Result<(String, String), rusqlite::Error> =
            stmt.query_row([], |row| Ok((row.get(0)?, row.get(1)?)));
        match res {
            Ok(result) => result,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                info!("No revisions found in database");
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }
    };

    info!("Found revision with id: {id}");

    let key = ObjectStoreKey::new(object_key);
    info!("Fetching revision data from object store with key: {key}");
    let start_time = std::time::Instant::now();
    let res = ts.object_store.get(&key).await?;
    info!(
        "Got response (content_type {:?}), now fetching bytes",
        res.content_type()
    );
    let bytes = res.bytes().await?;
    let duration = start_time.elapsed();
    info!("Fetching revision data took {duration:?}");

    info!("Deserializing revision data");
    let pak: Pak =
        facet_json::from_str(std::str::from_utf8(&bytes[..])?).map_err(|e| e.into_owned())?;
    Ok(Some(pak))
}

pub async fn serve(args: MomServeArgs) -> eyre::Result<()> {
    let MomServeArgs {
        config,
        web,
        tenants,
        listener,
    } = args;

    log::info!(
        "Serving with {} tenants: {}",
        tenants.len(),
        tenants.keys().join(", ")
    );

    // compute initial global state
    {
        let (tx_event, rx_event) = broadcast::channel(16);
        drop(rx_event);

        let mut gs = MomGlobalState {
            client: Arc::from(libhttpclient::load().client()),
            bx_event: tx_event,
            tenants: Default::default(),
            config: Arc::new(config),
            web,
        };

        for (tn, mut ti) in tenants {
            log::info!("Setting up tenant {}", tn.blue());

            let object_store = derivations::objectstore_for_tenant(&ti, gs.web.env).await?;
            let tn_for_sponsors = tn.clone();

            let mut pak: Option<Pak> = None;
            if let Some(rc) = ti.tc.rc_for_dev.take() {
                // make a dummy pak with the initial / dev revision config,
                // which contains useful things like the admin patreon/github IDs
                pak = Some(Pak {
                    id: RevisionId::new("dummy".to_string()),
                    inputs: Default::default(),
                    pages: Default::default(),
                    templates: Default::default(),
                    media_props: Default::default(),
                    svg_font_face_collection: Default::default(),
                    rc,
                })
            }

            let ts = MomTenantState {
                pool: mom_db_pool(&ti).unwrap(),
                users_inflight: InflightSlots::new(move |_| {
                    let gs = global_state();
                    log::info!(
                        "Grabbing sponsors inflight for tenant {}; gs has {} tenants",
                        tn_for_sponsors.blue(),
                        gs.tenants.len().yellow()
                    );
                    let ts = gs
                        .tenants
                        .get(&tn_for_sponsors)
                        .cloned()
                        .ok_or_else(|| {
                            eyre::eyre!(
                                "Tenant not found in global state: global state has tenants {}",
                                gs.tenants.keys().join(", ")
                            )
                        })
                        .unwrap();
                    Box::pin(async move {
                        let res = users::refresh_sponsors(&ts).await?;
                        ts.broadcast_event(TenantEventPayload::UsersUpdated(res.clone()))?;

                        Ok(res)
                    })
                }),
                users: Default::default(),
                pak: Arc::new(Mutex::new(pak)),
                object_store,
                ti: Arc::new(ti),
                transcode_jobs: Default::default(),
                derive_jobs: Default::default(),
            };

            eprintln!(
                "Inserting tenant {}, base dir is {}",
                ts.ti.tc.name.blue(),
                ts.ti.base_dir.red()
            );
            gs.tenants.insert(ts.ti.tc.name.clone(), Arc::new(ts));
        }

        eprintln!("Setting global state with {} tenants", gs.tenants.len());
        if GLOBAL_STATE.set(Box::leak(Box::new(gs))).is_err() {
            panic!("global state was already set? that's not good")
        }
    };

    eprintln!("Restoring all users from db...");
    for ts in global_state().tenants.values() {
        // try to load users from the database
        match users::fetch_all_users(&ts.pool) {
            Ok(users) => {
                eprintln!(
                    "{} Loaded {} users",
                    ts.ti.tc.name.magenta(),
                    users.users.len()
                );
                *ts.users.lock() = Some(users);
            }
            Err(e) => {
                error!(
                    "{} Failed to restore users from DB: {e}",
                    ts.ti.tc.name.magenta()
                );
            }
        }
    }

    // load the latest revision from the database for each tenant
    for (_, ts) in global_state().tenants.iter() {
        match load_revision_from_db(ts).await {
            Ok(Some(revision)) => {
                *ts.pak.lock() = Some(revision);
                log::debug!(
                    "Loaded latest revision from database for tenant {}",
                    ts.ti.tc.name
                );
            }
            Ok(None) => {
                log::debug!("No revision found in database for tenant {}", ts.ti.tc.name);
            }
            Err(e) => {
                log::error!(
                    "Failed to load revision from database for tenant {}: {e}",
                    ts.ti.tc.name
                );
            }
        }
    }

    // refresh sponsors regularly
    for ts in global_state().tenants.values().cloned() {
        tokio::spawn(async move {
            let tenant_name = ts.ti.tc.name.as_str();
            let interval = Duration::from_secs(120);

            loop {
                tokio::time::sleep(interval).await;
                match ts.users_inflight.query(()).await {
                    Ok(users) => {
                        log::debug!("[{}] Fetched {} sponsors", tenant_name, users.users.len());
                        *ts.users.lock() = Some(users);
                    }
                    Err(e) => {
                        log::debug!("[{tenant_name}] Failed to fetch sponsors: {e} / {e:?}")
                    }
                }
            }
        });
    }

    debug!("🐻 mom is now serving on {} 💅", listener.local_addr()?);
    endpoints::serve(listener).await
}
