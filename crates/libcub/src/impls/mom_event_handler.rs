use std::sync::Arc;

use config_types::{WebConfig, is_development};
use conflux::{Pak, PathMappings};
use cub_types::CubTenant;
use mom_types::{AllUsers, MomEvent, TenantEventPayload};
use tokio::sync::mpsc;

use super::{global_state, types::CubTenantImpl};

pub(crate) fn spawn_mom_event_handler(mut mev_rx: mpsc::Receiver<MomEvent>, web: WebConfig) {
    tokio::spawn(async move {
        loop {
            let ev = mev_rx.recv().await.unwrap();
            match ev {
                MomEvent::GoodMorning(_gm) => {
                    log::warn!(
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
                            log::warn!("Got message for unknown tenant {tn}");
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
        TenantEventPayload::UsersUpdated(users) => {
            handle_users_updated(ts, users);
        }
        TenantEventPayload::RevisionChanged(pak) => {
            handle_revision_changed(ts, pak, web).await;
        }
    }
}

fn handle_users_updated(ts: Arc<CubTenantImpl>, users: Arc<AllUsers>) {
    *ts.users.write() = users;
}

async fn handle_revision_changed(ts: Arc<CubTenantImpl>, pak: Box<Pak>, web: WebConfig) {
    if is_development() {
        log::info!("Received a pak from mom, ignoring since we're in development");
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
                log::error!("Failed to load revision: {e}");
                return;
            }
        }
    };
    ts.switch_to(rev);
}
