use std::collections::HashSet;

use futures_util::TryFutureExt;
use libgithub::GitHubCredentials;
use libhttpclient::HttpClient;
use merde::{CowStr, IntoStatic};

use crate::{
    Sponsors,
    impls::{MomTenantState, global_state},
};

pub(crate) async fn get_sponsors(ts: &MomTenantState) -> eyre::Result<Sponsors<'static>> {
    let client = global_state().client.clone();

    let (gh_sponsors, patreon_sponsors) = futures_util::future::try_join(
        github_list_sponsors(ts, client.as_ref()).map_err(|e| e.wrap_err("github_list_sponsors")),
        patreon_list_sponsors(ts, client.as_ref()).map_err(|e| e.wrap_err("patreon_list_sponsors")),
    )
    .await?;

    let sponsors = gh_sponsors
        .into_iter()
        .chain(patreon_sponsors.into_iter())
        .collect();

    Ok(Sponsors { sponsors })
}

async fn patreon_list_sponsors(
    ts: &MomTenantState,
    client: &dyn HttpClient,
) -> eyre::Result<HashSet<CowStr<'static>>> {
    let patreon = patreon::load();
    let rc = ts.rc()?;
    patreon.list_sponsors(&rc, client, &ts.pool).await
}

async fn github_list_sponsors(
    ts: &MomTenantState,
    client: &dyn HttpClient,
) -> eyre::Result<HashSet<CowStr<'static>>> {
    let github_credentials: String = {
        let conn = ts.pool.get()?;
        let mut stmt = conn.prepare(
            "
            SELECT data
            FROM github_credentials
            WHERE github_id = ?1
            ",
        )?;
        let creator_github_id = {
            let pak = ts.pak.lock();
            pak.as_ref()
                .and_then(|pak| pak.rc.admin_github_ids.first().cloned())
                .ok_or_else(|| eyre::eyre!("admin_github_ids should have at least one element"))?
        };
        stmt.query_row([creator_github_id], |row| row.get(0))
            .map_err(|e| {
                eyre::eyre!("rusqlite error: creator needs to log in with GitHub first: {e}")
            })?
    };
    let github_credentials = merde::json::from_str_owned::<GitHubCredentials>(&github_credentials)
        .map_err(|e| e.into_static())?;
    let github = libgithub::load();
    github
        .list_sponsors(&ts.ti.tc, client, &github_credentials)
        .await
}
