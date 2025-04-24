use std::sync::Arc;

use config_types::{WebConfig, is_development};
use conflux::{Pak, PathMappings};
use cub_types::CubTenant;
use mom_types::{MomEvent, Sponsors, TenantEventPayload};
use tokio::sync::mpsc;

use super::{global_state, types::CubTenantImpl};

pub(crate) fn spawn_mom_event_handler(mut mev_rx: mpsc::Receiver<MomEvent>, web: WebConfig) {
    tokio::spawn(async move {
        loop {
            let ev = mev_rx.recv().await.unwrap();
            match ev {
                MomEvent::GoodMorning(_gm) => {
                    tracing::warn!(
                        "Received a good morning later than expected. Probably we got reconnected."
                    );
                }
                MomEvent::TenantEvent(ev) => {
                    let tn = &ev.tenant_name;
                    let ts = match global_state::global_state()
                        .dynamic
                        .read()
                        .tenants_by_name
                        .get(tn)
                        .cloned()
                    {
                        Some(ts) => ts,
                        None => {
                            tracing::warn!("Got message for unknown tenant {tn}");
                            continue;
                        }
                    };

                    handle_tenant_event(ts, ev.payload, web).await;
                }
            }
        }
    });
}

async fn handle_tenant_event(
    ts: Arc<CubTenantImpl>,
    payload: mom_types::TenantEventPayload,
    web: WebConfig,
) {
    match payload {
        TenantEventPayload::SponsorsUpdated(sponsors) => {
            handle_sponsors_updated(ts, sponsors);
        }
        TenantEventPayload::RevisionChanged(pak) => {
            handle_revision_changed(ts, pak, web).await;
        }
    }
}

fn handle_sponsors_updated(ts: Arc<CubTenantImpl>, sponsors: Sponsors) {
    *ts.sponsors.write() = Arc::new(sponsors);
}

async fn handle_revision_changed(ts: Arc<CubTenantImpl>, pak: Box<Pak>, web: WebConfig) {
    if is_development() {
        tracing::info!("Received a pak from mom, ignoring since we're in development");
        return;
    }

    let rev = {
        let prev_rev = ts.rev().ok();
        let mappings = PathMappings::from_ti(ts.ti());
        let mod_revision = librevision::load();
        match mod_revision
            .load_pak(
                *pak,
                ts.ti().clone(),
                prev_rev.as_ref().map(|rev| rev.rev.as_ref()),
                mappings,
                web,
            )
            .await
        {
            Ok(lrev) => lrev,
            Err(e) => {
                tracing::error!("Failed to load revision: {e}");
                return;
            }
        }
    };
    ts.switch_to(rev);
}
