use std::collections::HashMap;

use axum::routing::get;
use config_types::is_development;
use libhttpclient::Uri;

use crate::impls::{MomTenantState, global_state};
use axum::{Extension, Router};
use axum::{
    body::Bytes,
    extract::Path,
    http::StatusCode,
    routing::{post, put},
};
use credentials::AuthBundle;
use libgithub::{GitHubCallbackArgs, GitHubCallbackResponse, GitHubCredentials};
use libpatreon::{
    ForcePatreonRefresh, PatreonCallbackArgs, PatreonCallbackResponse, PatreonCredentials,
    PatreonRefreshCredentials, PatreonRefreshCredentialsArgs, PatreonStore,
};
use merde::IntoStatic;
use mom_types::{ListMissingArgs, ListMissingResponse, TenantEventPayload};
use objectstore_types::{ObjectStoreKey, ObjectStoreKeyRef};

use crate::impls::site::{HttpError, IntoReply, MerdeJson, Reply};

use super::tenant_extractor::TenantExtractor;

mod derive;
mod media;

pub fn tenant_routes() -> Router {
    Router::new()
        .route("/patreon/callback", post(patreon_callback))
        .route(
            "/patreon/refresh-credentials",
            post(patreon_refresh_credentials),
        )
        .route("/github/callback", post(github_callback))
        .route("/auth-bundle/update", post(auth_bundle_update))
        .route("/objectstore/list-missing", post(objectstore_list_missing))
        .route("/objectstore/put/*key", put(objectstore_put_key))
        .route("/media/upload", get(media::upload))
        .route("/media/transcode", post(media::transcode))
        .route("/derive", post(derive::derive))
        .route("/revision/upload/:revision_id", put(revision_upload_revid))
}

async fn patreon_callback(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: PatreonCallbackArgs = merde::json::from_str(body)?;

    let mod_patreon = libpatreon::load();
    let pool = &ts.pool;

    let creds = mod_patreon
        .handle_oauth_callback(&ts.ti.tc, global_state().web, &args)
        .await?;
    let res: Option<PatreonCallbackResponse> = match creds {
        Some(creds) => {
            let (pat_creds, auth_bundle) = mod_patreon
                .to_auth_bundle(
                    &ts.ti.tc,
                    &ts.rc()?,
                    creds,
                    pool,
                    ForcePatreonRefresh::DontForceRefresh,
                )
                .await?;

            let patreon_id = auth_bundle.user_info.profile.patreon_id.as_deref().unwrap();
            pool.save_patreon_credentials(patreon_id, &pat_creds)?;
            Some(PatreonCallbackResponse { auth_bundle })
        }
        None => None,
    };
    MerdeJson(res).into_reply()
}

async fn patreon_refresh_credentials(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let args =
        merde::json::from_str::<PatreonRefreshCredentialsArgs>(std::str::from_utf8(&body[..])?)?;
    let patreon_id = args.patreon_id;

    let site_credentials = ts
        .patreon_creds_inflight
        .query(patreon_id.to_string())
        .await
        .map_err(|e| {
            tracing::error!("Failed to refresh patreon credentials: {}", e);
            HttpError::with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not refresh patreon credentials",
            )
        })?;

    MerdeJson(PatreonRefreshCredentials {
        auth_bundle: site_credentials,
    })
    .into_reply()
}

async fn github_callback(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: GitHubCallbackArgs = merde::json::from_str(body)?;

    let mod_github = libgithub::load();

    let web = global_state().web;
    let creds = mod_github
        .handle_oauth_callback(&ts.ti.tc, web, &args)
        .await?;

    let res: Option<GitHubCallbackResponse> = match creds {
        Some(creds) => {
            let rc = ts.rc()?;
            let (github_creds, site_creds) = mod_github.to_auth_bundle(&rc, web, creds).await?;

            // Save GitHub credentials to the database
            let conn = ts.pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO github_credentials (github_id, data) VALUES (?1, ?2)",
                rusqlite::params![
                    site_creds.user_info.profile.github_id,
                    merde::json::to_string(&github_creds)?
                ],
            )?;
            Some(GitHubCallbackResponse {
                auth_bundle: site_creds,
                github_credentials: github_creds,
            })
        }
        None => None,
    };
    MerdeJson(res).into_reply()
}

fn get_patreon_credentials(
    conn: &rusqlite::Connection,
    patreon_id: &str,
) -> Result<PatreonCredentials<'static>, HttpError> {
    let pat_creds_payload: String = conn
        .query_row(
            "SELECT data FROM patreon_credentials WHERE patreon_id = ?1",
            [patreon_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| {
            HttpError::with_status(
                StatusCode::UNAUTHORIZED,
                format!("No Patreon credentials found for user {patreon_id}"),
            )
        })?;

    merde::json::from_str::<PatreonCredentials>(&pat_creds_payload)
        .map(|creds| creds.into_static())
        .map_err(|_| {
            HttpError::with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to parse Patreon credentials",
            )
        })
}

fn get_github_credentials(
    conn: &rusqlite::Connection,
    github_id: &str,
) -> Result<GitHubCredentials<'static>, HttpError> {
    let github_creds: String = conn
        .query_row(
            "SELECT data FROM github_credentials WHERE github_id = ?1",
            [github_id],
            |row| row.get(0),
        )
        .map_err(|_| {
            HttpError::with_status(
                StatusCode::UNAUTHORIZED,
                format!("No GitHub credentials found for user {github_id}"),
            )
        })?;

    merde::json::from_str::<GitHubCredentials>(&github_creds)
        .map(|creds| creds.into_static())
        .map_err(|_| {
            HttpError::with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to parse GitHub credentials",
            )
        })
}

// #[axum::debug_handler]
async fn auth_bundle_update(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let auth_bundle: AuthBundle = merde::json::from_str(std::str::from_utf8(&body[..])?)?;

    let new_auth_bundle: AuthBundle = if let Some(patreon_id) =
        auth_bundle.user_info.profile.patreon_id
    {
        let mod_patreon = libpatreon::load();
        let pat_creds = {
            let conn = ts.pool.get()?;
            get_patreon_credentials(&conn, &patreon_id)?
        };

        let (_pat_creds, auth_bundle) = mod_patreon
            .to_auth_bundle(
                &ts.ti.tc,
                &ts.rc()?,
                pat_creds,
                &ts.pool,
                ForcePatreonRefresh::DontForceRefresh,
            )
            .await?;
        auth_bundle
    } else if let Some(github_id) = auth_bundle.user_info.profile.github_id {
        let mod_github = libgithub::load();
        let github_creds = {
            let conn = ts.pool.get()?;
            get_github_credentials(&conn, &github_id)?
        };

        let web = global_state().web;
        let rc = ts.rc()?;
        let (_gh_creds, auth_bundle) = mod_github.to_auth_bundle(&rc, web, github_creds).await?;
        auth_bundle
    } else {
        return HttpError::with_status(
            StatusCode::BAD_REQUEST,
            "AuthBundle must contain either a patreon_id or github_id",
        )
        .into_reply();
    };

    MerdeJson(new_auth_bundle).into_reply()
}

async fn objectstore_list_missing(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let args: ListMissingArgs = merde::json::from_str(std::str::from_utf8(&body[..])?)?;
    let mut conn = ts.pool.get()?;

    // first do a local lookup
    let mut missing = args.objects_to_query.clone();
    let mut had_those_locally: Vec<ObjectStoreKey> = Default::default();
    let keys = missing.keys().cloned().collect::<Vec<_>>();
    for key_chunk in keys.chunks(100) {
        let placeholders = (0..key_chunk.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "SELECT key FROM objectstore_entries WHERE key IN ({})",
            placeholders
        );

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(key_chunk), |row| {
            row.get::<_, ObjectStoreKey>(0)
        })?;

        for row in rows {
            let row = row?;
            missing.remove(&row);
            had_those_locally.push(row);
        }
    }

    // then, if we're in dev, do a remote lookup
    if is_development() {
        let args = ListMissingArgs {
            objects_to_query: args.objects_to_query.clone(),
            mark_these_as_uploaded: Some(had_those_locally.clone()),
        };
        let tenant_name = &ts.ti.tc.name;
        let production_uri = config_types::production_mom_url().parse::<Uri>().unwrap();

        let uri = Uri::builder()
            .scheme(production_uri.scheme().unwrap().clone())
            .authority(production_uri.authority().unwrap().clone())
            .path_and_query(format!("/tenant/{tenant_name}/objectstore/list-missing"))
            .build()
            .unwrap();
        let client = libhttpclient::load().client();

        match client.post(uri).json(&args)?.send_and_expect_200().await {
            Err(e) => {
                tracing::warn!("Failed to query production mom: {e}");
                tracing::warn!("...ignoring");
            }
            Ok(res) => {
                let remote_res = res
                    .json::<ListMissingResponse>()
                    .await
                    .map_err(|e| eyre::eyre!("Failed to parse production mom response: {e}"))?;

                // Calculate and insert the ones the remote had that we didn't
                let tx = conn.transaction()?;
                for remote_key in remote_res.missing.keys() {
                    if !had_those_locally.contains(remote_key) {
                        tx.execute(
                            "INSERT OR REPLACE INTO objectstore_entries (key) VALUES (?1)",
                            [remote_key],
                        )?;
                        missing.remove(remote_key);
                    }
                }
                tx.commit()?;
            }
        }
    }

    tracing::debug!(
        "{}/{} keys are missing: {:#?}",
        missing.len(),
        args.objects_to_query.len(),
        missing
    );

    MerdeJson(ListMissingResponse { missing }).into_reply()
}

async fn objectstore_put_key(
    Path(path): Path<HashMap<String, String>>,
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    payload: Bytes,
) -> Reply {
    let key = path
        .get("key")
        .cloned()
        .ok_or_else(|| eyre::eyre!("Missing key"))?;
    let key = ObjectStoreKeyRef::from_str(&key);
    let size = payload.len();
    tracing::debug!(%key, %size, "Putting asset into object store");

    // Upload to cloud storage
    let result = ts.object_store.put(key, payload).await?;
    tracing::debug!(e_tag = ?result.e_tag, "Uploaded to object store");

    // Insert into the database
    {
        let conn = ts.pool.get()?;
        conn.execute(
            "INSERT OR REPLACE INTO objectstore_entries (key) VALUES (?1)",
            [&key],
        )?;
    }

    // Return 200 if everything went fine
    StatusCode::OK.into_reply()
}

async fn revision_upload_revid(
    Path(path): Path<HashMap<String, String>>,
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    payload: Bytes,
) -> Reply {
    let revision_id = path
        .get("revision_id")
        .cloned()
        .ok_or_else(|| eyre::eyre!("Missing revision_id"))?;
    tracing::debug!(%revision_id, "Uploading revision package");

    // Load the revision from JSON
    let pak: conflux::Pak = merde::json::from_str(std::str::from_utf8(&payload)?)?;
    let pak = pak.into_static();

    // Spawn a background task to handle upload, DB insertion, and notification
    tokio::spawn(async move {
        let object_store = ts.object_store.clone();

        // Upload to cloud storage (for backup)
        let key = ObjectStoreKey::new(format!("revpaks/{}", revision_id));
        let result = object_store.put(&key, payload.clone()).await?;
        tracing::debug!(e_tag = ?result.e_tag, "Uploaded revision package to object store");

        // Insert into the database
        {
            let conn = ts.pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO revisions (id, object_key, uploaded_at) VALUES (?1, ?2, datetime('now'))",
                [&revision_id, &key.to_string()],
            )?;
        }

        // Store the revision in global state
        {
            *ts.pak.lock() = Some(pak.clone());
        }

        // Notify about the new revision
        ts.broadcast_event(TenantEventPayload::RevisionChanged(Box::new(pak)))?;

        Ok::<_, eyre::Report>(())
    });

    // Return 200 immediately after spawning the background task
    StatusCode::OK.into_reply()
}
