use std::collections::HashMap;

use axum::routing::get;
use config_types::is_development;
use credentials::UserId;
use libhttpclient::Uri;
use rusqlite::OptionalExtension;

use crate::impls::discord_roles::synchronize_one_discord_role;
use crate::impls::users::{
    fetch_user_info, save_discord_credentials, save_discord_profile, save_github_credentials,
    save_github_profile, save_patreon_credentials, save_patreon_profile,
};
use crate::impls::{MomTenantState, global_state};
use axum::{Extension, Router};
use axum::{
    body::Bytes,
    extract::Path,
    http::StatusCode,
    routing::{post, put},
};
use libgithub::GithubCallbackArgs;
use libpatreon::PatreonCallbackArgs;
use mom_types::{
    GithubCallbackResponse, ListMissingArgs, ListMissingResponse, PatreonCallbackResponse,
    RefreshProfileArgs, TenantEventPayload,
};
use objectstore_types::{ObjectStoreKey, ObjectStoreKeyRef};

use crate::impls::site::{FacetJson, IntoReply, Reply};

use super::tenant_extractor::TenantExtractor;

mod derive;
mod media;
mod opendoor;

pub fn tenant_routes() -> Router {
    Router::new()
        .route("/patreon/callback", post(patreon_callback))
        .route("/github/callback", post(github_callback))
        .route("/discord/callback", post(discord_callback))
        .route("/patreon/unlink", post(patreon_unlink))
        .route("/github/unlink", post(github_unlink))
        .route("/discord/unlink", post(discord_unlink))
        .route("/refresh-userinfo", post(refresh_userinfo))
        .route("/make-api-key", post(make_api_key))
        .route("/verify-api-key", post(verify_api_key))
        .route("/objectstore/list-missing", post(objectstore_list_missing))
        .route("/objectstore/put/{*key}", put(objectstore_put_key))
        .route("/media/upload", get(media::upload))
        .route("/media/transcode", post(media::transcode))
        .route("/derive", post(derive::derive))
        .route("/revision/upload/{revision_id}", put(revision_upload_revid))
        .route("/opendoor", post(opendoor::opendoor))
}

async fn patreon_callback(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: PatreonCallbackArgs = facet_json::from_str(body)?;

    let mod_patreon = libpatreon::load();
    let pool = &ts.pool;

    let client = global_state().client.as_ref();

    let creds = mod_patreon
        .handle_oauth_callback(&ts.ti.tc, global_state().web, &args, client)
        .await?;

    let res: Option<PatreonCallbackResponse> = match creds {
        Some(creds) => {
            let profile = mod_patreon.fetch_profile(&ts.rc()?, &creds, client).await?;
            save_patreon_credentials(pool, &profile.id, &creds)?;

            let conn = pool.get()?;

            let user_id = if let Some(logged_in_user_id) = args.logged_in_user_id {
                // If we're already logged in, use that user ID
                logged_in_user_id
            } else {
                // Try to find an existing user by querying the patreon_profiles table
                let existing_user: Option<i64> = conn
                    .query_row(
                        "SELECT user_id FROM patreon_profiles WHERE id = ?1",
                        [&profile.id],
                        |row| row.get(0),
                    )
                    .optional()?;

                if let Some(existing_user_id) = existing_user {
                    // Found an existing user with this patreon profile
                    UserId::new(existing_user_id.to_string())
                } else {
                    // No existing user, create a new one
                    use crate::impls::users::create_user;
                    create_user(pool)?
                }
            };

            save_patreon_profile(pool, &profile, &user_id)?;
            let user_info = { fetch_user_info(pool, &user_id)?.unwrap() };

            Some(PatreonCallbackResponse { user_info })
        }
        None => None,
    };
    FacetJson(res).into_reply()
}

async fn github_callback(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: GithubCallbackArgs = facet_json::from_str(body)?;

    let pool = &ts.pool;
    let mod_github = libgithub::load();

    let web = global_state().web;
    let creds = mod_github
        .handle_oauth_callback(&ts.ti.tc, web, &args)
        .await?;
    let client = global_state().client.as_ref();

    let res: Option<GithubCallbackResponse> = match creds {
        Some(creds) => {
            let profile = mod_github.fetch_profile(&creds, client).await?;
            save_github_credentials(pool, &profile.id, &creds)?;

            let conn = pool.get()?;

            let user_id = if let Some(logged_in_user_id) = args.logged_in_user_id {
                // If we're already logged in, use that user ID
                logged_in_user_id
            } else {
                // Try to find an existing user by querying the github_profiles table
                let existing_user: Option<i64> = conn
                    .query_row(
                        "SELECT user_id FROM github_profiles WHERE id = ?1",
                        [&profile.id],
                        |row| row.get(0),
                    )
                    .optional()?;

                if let Some(existing_user_id) = existing_user {
                    // Found an existing user with this github profile
                    UserId::new(existing_user_id.to_string())
                } else {
                    // No existing user, create a new one
                    use crate::impls::users::create_user;
                    create_user(pool)?
                }
            };

            save_github_profile(pool, &profile, &user_id)?;
            let user_info = { fetch_user_info(pool, &user_id)?.unwrap() };

            Some(GithubCallbackResponse {
                user_info,
                scope: creds.scope.clone(),
            })
        }
        None => None,
    };
    FacetJson(res).into_reply()
}

async fn discord_callback(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: libdiscord::DiscordCallbackArgs = facet_json::from_str(body)?;

    let pool = &ts.pool;
    let mod_discord = libdiscord::load();

    let web = global_state().web;
    let creds = mod_discord
        .handle_oauth_callback(&ts.ti.tc, web, &args)
        .await?;

    let res: Option<mom_types::DiscordCallbackResponse> = match creds {
        Some(creds) => {
            let profile = mod_discord.fetch_profile(&creds).await?;
            save_discord_credentials(pool, &profile.id, &creds)?;

            let conn = pool.get()?;

            let user_id = if let Some(logged_in_user_id) = args.logged_in_user_id {
                // If we're already logged in, use that user ID
                logged_in_user_id
            } else {
                // Try to find an existing user by querying the discord_profiles table
                let existing_user: Option<i64> = conn
                    .query_row(
                        "SELECT user_id FROM discord_profiles WHERE id = ?1",
                        [&profile.id],
                        |row| row.get(0),
                    )
                    .optional()?;

                if let Some(existing_user_id) = existing_user {
                    // Found an existing user with this discord profile
                    UserId::new(existing_user_id.to_string())
                } else {
                    // No existing user, create a new one
                    use crate::impls::users::create_user;
                    create_user(pool)?
                }
            };

            save_discord_profile(pool, &profile, &user_id)?;
            let user_info = { fetch_user_info(pool, &user_id)?.unwrap() };

            synchronize_one_discord_role(ts.as_ref(), &user_info).await?;

            Some(mom_types::DiscordCallbackResponse { user_info })
        }
        None => None,
    };
    FacetJson(res).into_reply()
}

async fn patreon_unlink(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: libpatreon::PatreonUnlinkArgs = facet_json::from_str(body)?;

    let pool = &ts.pool;
    let conn = pool.get()?;

    // Delete the patreon profile for this user
    conn.execute(
        "DELETE FROM patreon_profiles WHERE user_id = ?1",
        [&args.logged_in_user_id],
    )?;

    // Return fresh user info
    use crate::impls::users::refresh_userinfo;
    let user_info = refresh_userinfo(&ts, &args.logged_in_user_id).await?;

    FacetJson(user_info).into_reply()
}

async fn github_unlink(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: libgithub::GithubUnlinkArgs = facet_json::from_str(body)?;

    let pool = &ts.pool;
    let conn = pool.get()?;

    // Delete the github profile for this user
    conn.execute(
        "DELETE FROM github_profiles WHERE user_id = ?1",
        [&args.logged_in_user_id],
    )?;

    // Return fresh user info
    use crate::impls::users::refresh_userinfo;
    let user_info = refresh_userinfo(&ts, &args.logged_in_user_id).await?;

    FacetJson(user_info).into_reply()
}

async fn discord_unlink(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: libdiscord::DiscordUnlinkArgs = facet_json::from_str(body)?;

    let pool = &ts.pool;
    let conn = pool.get()?;

    // Delete the discord profile for this user
    conn.execute(
        "DELETE FROM discord_profiles WHERE user_id = ?1",
        [&args.logged_in_user_id],
    )?;

    // Return fresh user info
    use crate::impls::users::refresh_userinfo;
    let user_info = refresh_userinfo(&ts, &args.logged_in_user_id).await?;

    FacetJson(user_info).into_reply()
}

// #[axum::debug_handler]
async fn refresh_userinfo(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: RefreshProfileArgs = facet_json::from_str(body)?;

    use crate::impls::users::refresh_userinfo;
    let user_info = refresh_userinfo(&ts, &args.user_id).await?;

    FacetJson(user_info).into_reply()
}

async fn objectstore_list_missing(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let args: ListMissingArgs = facet_json::from_str(std::str::from_utf8(&body[..])?)?;
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
        let query = format!("SELECT key FROM objectstore_entries WHERE key IN ({placeholders})");

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
                log::warn!("Failed to query production mom: {e}");
                log::warn!("...ignoring");
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

    log::debug!(
        "{}/{} keys are missing: {:#?}",
        missing.len(),
        args.objects_to_query.len(),
        missing
    );

    FacetJson(ListMissingResponse { missing }).into_reply()
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
    log::debug!("Putting asset into object store: key={key}, size={size}",);

    // Upload to cloud storage
    let result = ts.object_store.put(key, payload).await?;
    log::debug!("Uploaded to object store. e_tag={:?}", result.e_tag);

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
    log::debug!("Uploading revision package; revision_id={revision_id}");

    // Load the revision from JSON
    let pak: conflux::Pak = facet_json::from_str(std::str::from_utf8(&payload)?)?;

    // Spawn a background task to handle upload, DB insertion, and notification
    tokio::spawn(async move {
        let object_store = ts.object_store.clone();

        // Upload to cloud storage (for backup)
        let key = ObjectStoreKey::new(format!("revpaks/{revision_id}"));
        let result = object_store.put(&key, payload.clone()).await?;
        log::debug!(
            "Uploaded revision package to object store, e_tag={:?}",
            result.e_tag
        );

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

async fn make_api_key(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: mom_types::MakeApiKeyArgs = facet_json::from_str(body)?;

    use crate::impls::users::make_api_key;
    let api_key = make_api_key(&ts.pool, &args.user_id)?;

    FacetJson(mom_types::MakeApiKeyResponse { api_key }).into_reply()
}

async fn verify_api_key(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: mom_types::VerifyApiKeyArgs = facet_json::from_str(body)?;

    use crate::impls::users::verify_api_key;
    let user_info = verify_api_key(&ts.pool, &args.api_key)?;

    FacetJson(mom_types::VerifyApiKeyResponse { user_info }).into_reply()
}
