use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, OnceLock},
    time::Duration,
};

use axum::extract::ws;
use config_types::{
    MomConfig, RevisionConfig, TenantDomain, TenantInfo, WebConfig, is_development,
};
use conflux::Pak;
use credentials::AuthBundle;
use inflight::InflightSlots;
use itertools::Itertools;
use libhttpclient::HttpClient;
use libobjectstore::ObjectStore;
use libpatreon::{ForcePatreonRefresh, PatreonCredentials, PatreonStore, test_patreon_renewal};
use merde::IntoStatic;
use mom_types::Sponsors;
use objectstore_types::ObjectStoreKey;
use owo_colors::OwoColorize;
use parking_lot::Mutex;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

use crate::{
    DeriveJobInfo, DeriveParams, MomEvent, MomServeArgs, TenantEvent, TenantEventPayload,
    TranscodeJobInfo, TranscodeParams, impls::db::mom_db_pool,
};

mod db;
mod deriver;
mod endpoints;
mod ffmpeg;
mod ffmpeg_stream;
mod site;
mod sponsors;

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

    pub(crate) patreon_creds_inflight: InflightSlots<String, AuthBundle<'static>>,
    pub(crate) sponsors_inflight: InflightSlots<(), Sponsors<'static>>,
    pub(crate) sponsors: Arc<Mutex<Option<Sponsors<'static>>>>,

    pub(crate) pak: Arc<Mutex<Option<Pak<'static>>>>,

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
    pub(crate) fn event_to_message(event: MomEvent<'static>) -> ws::Message {
        let json_string = merde::json::to_string(&event).unwrap();
        ws::Message::Text(json_string)
    }

    pub(crate) fn broadcast_event(&self, event: MomEvent<'static>) -> eyre::Result<()> {
        let ev_debug = format!("{event:?}");
        let event = merde::json::to_string(&event)?;
        match self.bx_event.send(event) {
            Ok(n) => tracing::info!("Broadcast to {n} subscribers: {ev_debug}"),
            Err(_) => tracing::info!("No subscribers for event: {ev_debug}"),
        }

        Ok(())
    }
}

impl MomTenantState {
    pub(crate) fn broadcast_event(&self, payload: TenantEventPayload<'static>) -> eyre::Result<()> {
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

impl PatreonStore for Pool {
    fn fetch_patreon_credentials(
        &self,
        patreon_id: &str,
    ) -> eyre::Result<Option<PatreonCredentials<'static>>> {
        let conn = self.get()?;
        let mut stmt = conn.prepare(
            "
                SELECT data
                FROM patreon_credentials
                WHERE patreon_id = ?1
            ",
        )?;
        let creds: Option<String> = stmt.query_row([patreon_id], |row| row.get(0))?;
        if let Some(creds) = creds {
            Ok(Some(
                merde::json::from_str_owned::<PatreonCredentials>(&creds)
                    .map_err(|e| e.into_static())?,
            ))
        } else {
            Ok(None)
        }
    }

    fn save_patreon_credentials(
        &self,
        patreon_id: &str,
        credentials: &PatreonCredentials,
    ) -> eyre::Result<()> {
        let conn = self.get()?;
        conn.execute(
            "
                INSERT INTO patreon_credentials (patreon_id, data)
                VALUES (?1, ?2)
                ON CONFLICT (patreon_id) DO UPDATE SET data = ?2
                ",
            rusqlite::params![patreon_id, merde::json::to_string(credentials)?],
        )?;
        Ok(())
    }
}

pub(crate) async fn patreon_refresh_credentials_inner(
    ts: Arc<MomTenantState>,
    patreon_id: String,
) -> Result<AuthBundle<'static>, eyre::Report> {
    let pool = &ts.pool;
    let mod_patreon = patreon::load();

    let pat_creds = pool
        .fetch_patreon_credentials(&patreon_id)?
        .ok_or_else(|| eyre::eyre!("Could not find patreon credentials for {patreon_id}"))?;

    info!("Refreshing patreon credentials");

    let mut refresh_creds = pat_creds.into_static();
    if is_development() && test_patreon_renewal() {
        refresh_creds.access_token = "bad-token-for-testing".into()
    }

    let (_pat_creds, site_creds) = mod_patreon
        .to_auth_bundle(
            &ts.ti.tc,
            &ts.rc()?,
            refresh_creds,
            pool,
            ForcePatreonRefresh::ForceRefresh,
        )
        .await?;

    Ok(site_creds)
}

pub(crate) fn save_sponsors_to_db(
    ts: &MomTenantState,
    sponsors: Sponsors<'static>,
) -> eyre::Result<()> {
    let conn = ts.pool.get()?;

    // delete old entries
    conn.execute(
        "DELETE FROM sponsors WHERE created_at < datetime('now', '-2 hours')",
        [],
    )?;

    // insert new entry
    conn.execute(
        "INSERT INTO sponsors (sponsors_json) VALUES (?1)",
        [merde::json::to_string(&sponsors)?],
    )?;

    Ok(())
}

pub(crate) fn load_sponsors_from_db(
    ts: &MomTenantState,
) -> eyre::Result<Option<Sponsors<'static>>> {
    let conn = ts.pool.get()?;
    let mut stmt = conn.prepare(
        "
            SELECT sponsors_json
            FROM sponsors
            ORDER BY created_at DESC
            LIMIT 1
            ",
    )?;
    let res: Result<String, rusqlite::Error> = stmt.query_row([], |row| row.get(0));
    match res {
        Ok(sponsors_json) => Ok(Some(
            merde::json::from_str_owned::<Sponsors>(&sponsors_json).map_err(|e| e.into_static())?,
        )),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub(crate) async fn load_revision_from_db(
    ts: &MomTenantState,
) -> eyre::Result<Option<Pak<'static>>> {
    debug!("Loading latest revision from database");
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
                debug!("No revisions found in database");
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }
    };

    debug!("Found revision with id: {id}");

    let key = ObjectStoreKey::new(object_key);
    debug!("Fetching revision data from object store with key: {key}");
    let start_time = std::time::Instant::now();
    let res = ts.object_store.get(&key).await?;
    debug!(
        "Got response (content_type {:?}), now fetching bytes",
        res.content_type()
    );
    let bytes = res.bytes().await?;
    let duration = start_time.elapsed();
    debug!("Fetching revision data took {duration:?}");

    debug!("Deserializing revision data");
    let revision: Pak = merde::json::from_str_owned(std::str::from_utf8(&bytes[..])?)
        .map_err(|e| e.into_static())?;
    Ok(Some(revision))
}

pub async fn serve(args: MomServeArgs) -> noteyre::Result<()> {
    let MomServeArgs {
        config,
        web,
        tenants,
        listener,
    } = args;

    // compute initial global state
    {
        let (tx_event, rx_event) = broadcast::channel(16);
        drop(rx_event);

        let mut gs = MomGlobalState {
            client: Arc::from(httpclient::load().client()),
            bx_event: tx_event,
            tenants: Default::default(),
            config: Arc::new(config),
            web,
        };

        for (tn, ti) in tenants {
            eprintln!("Setting up tenant {}", tn.blue());

            let object_store = derivations::objectstore_for_tenant(&ti, gs.web.env).await?;
            let tn_for_creds = tn.clone();
            let tn_for_sponsors = tn.clone();

            let ts = MomTenantState {
                pool: mom_db_pool(&ti).unwrap(),
                patreon_creds_inflight: InflightSlots::new(move |k: &String| {
                    let ts = global_state().tenants.get(&tn_for_creds).cloned().unwrap();
                    Box::pin(patreon_refresh_credentials_inner(ts, k.clone()))
                }),
                sponsors_inflight: InflightSlots::new(move |_| {
                    let gs = global_state();
                    eprintln!(
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
                        let res = sponsors::get_sponsors(&ts).await?;
                        if let Err(e) = save_sponsors_to_db(ts.as_ref(), res.clone()) {
                            tracing::error!("Failed to save sponsors to DB: {e}")
                        }
                        ts.broadcast_event(TenantEventPayload::SponsorsUpdated(res.clone()))?;

                        Ok(res)
                    })
                }),
                sponsors: Default::default(),
                pak: Default::default(),
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

    eprintln!("Trying to load all sponsors from db...");
    for ts in global_state().tenants.values() {
        // try to load sponsors from the database
        match load_sponsors_from_db(ts.as_ref()) {
            Ok(Some(sponsors)) => {
                eprintln!(
                    "{} Loaded {} sponsors",
                    ts.ti.tc.name.magenta(),
                    sponsors.sponsors.len()
                );
                *ts.sponsors.lock() = Some(sponsors);
            }
            Ok(None) => {
                eprintln!("{} No sponsors found in DB", ts.ti.tc.name.magenta());
            }
            Err(e) => {
                error!(
                    "{} Failed to restore sponsors from DB: {e}",
                    ts.ti.tc.name.magenta()
                );
            }
        }
    }

    // refresh sponsors regularly
    for ts in global_state().tenants.values().cloned() {
        tokio::spawn(async move {
            let interval = Duration::from_secs(120);
            let tenant_name = ts.ti.tc.name.as_str();
            if ts.sponsors.lock().is_some() {
                tokio::time::sleep(interval).await;
            }

            loop {
                match ts.sponsors_inflight.query(()).await {
                    Ok(sponsors) => {
                        tracing::debug!(
                            "[{}] Fetched {} sponsors",
                            tenant_name,
                            sponsors.sponsors.len()
                        );
                        *ts.sponsors.lock() = Some(sponsors);
                    }
                    Err(e) => {
                        tracing::debug!("[{}] Failed to fetch sponsors: {e} / {e:?}", tenant_name)
                    }
                }
                tokio::time::sleep(interval).await;
            }
        });
    }

    // load the latest revision from the database for each tenant
    for (_, ts) in global_state().tenants.iter() {
        match load_revision_from_db(ts).await {
            Ok(Some(revision)) => {
                *ts.pak.lock() = Some(revision);
                tracing::debug!(
                    "Loaded latest revision from database for tenant {}",
                    ts.ti.tc.name
                );
            }
            Ok(None) => {
                tracing::debug!("No revision found in database for tenant {}", ts.ti.tc.name);
            }
            Err(e) => {
                tracing::error!(
                    "Failed to load revision from database for tenant {}: {e}",
                    ts.ti.tc.name
                );
            }
        }
    }

    debug!("🐻 mom is now serving on {} 💅", listener.local_addr()?);
    endpoints::serve(listener).await.bs()
}
