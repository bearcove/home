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
use libgithub::{GitHubCallbackArgs, GitHubCallbackResponse, GithubCredentials};
use libpatreon::{
    ForcePatreonRefresh, PatreonCallbackArgs, PatreonCallbackResponse, PatreonCredentials,
    PatreonRefreshCredentials, PatreonRefreshCredentialsArgs, PatreonStore,
};
use mom_types::{ListMissingArgs, ListMissingResponse, TenantEventPayload};
use objectstore_types::{ObjectStoreKey, ObjectStoreKeyRef};
use time::{Duration, OffsetDateTime};

use crate::impls::site::{FacetJson, HttpError, IntoReply, Reply};

use super::tenant_extractor::TenantExtractor;

mod derive;
mod email_login;
mod media;

pub fn tenant_routes() -> Router {
    Router::new()
        .route("/patreon/callback", post(patreon_callback))
        .route("/github/callback", post(github_callback))
        .route("/refresh-profile", post(refresh_profile))
        .route("/objectstore/list-missing", post(objectstore_list_missing))
        .route("/objectstore/put/{*key}", put(objectstore_put_key))
        .route("/media/upload", get(media::upload))
        .route("/media/transcode", post(media::transcode))
        .route("/derive", post(derive::derive))
        .route("/revision/upload/{revision_id}", put(revision_upload_revid))
        .route(
            "/email/generate-code",
            post(email_login::generate_login_code),
        )
        .route(
            "/email/validate-code",
            post(email_login::validate_login_code),
        )
}

fn save_patreon_credentials(
    pool: &Pool,
    patreon_id: &str,
    credentials: &PatreonCredentials,
) -> eyre::Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT OR REPLACE INTO patreon_credentials (id, access_token, refresh_token, expires_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            patreon_id,
            credentials.access_token,
            credentials.refresh_token,
            credentials.expires_at
        ],
    )?;
    Ok(())
}

use rusqlite::Pool;

fn save_patreon_profile(pool: &Pool, profile: &PatreonProfile) -> eyre::Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT OR REPLACE INTO patreon_profiles (id, tier, full_name, thumb_url, updated_at) VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)",
        rusqlite::params![
            profile.id,
            profile.tier,
            profile.full_name,
            profile.thumb_url
        ],
    )?;
    Ok(())
}

fn fetch_user_info(pool: &Pool, user_id: &str) -> eyre::Result<Option<UserInfo>> {
    let conn = pool.get()?;

    // First, fetch the user record
    let user_row: Option<(String, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT id, patreon_user_id, github_user_id FROM users WHERE id = ?1",
            [user_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?.to_string(),
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;

    let Some((id, patreon_user_id, github_user_id)) = user_row else {
        return Ok(None);
    };

    // Fetch Patreon profile if linked
    let patreon = if let Some(patreon_id) = patreon_user_id {
        conn.query_row(
            "SELECT id, tier, full_name, thumb_url FROM patreon_profiles WHERE id = ?1",
            [&patreon_id],
            |row| {
                Ok(PatreonProfile {
                    id: row.get(0)?,
                    tier: row.get(1)?,
                    full_name: row.get(2)?,
                    thumb_url: row.get(3)?,
                })
            },
        )
        .optional()?
    } else {
        None
    };

    // Fetch GitHub profile if linked
    let github = if let Some(github_id) = github_user_id {
        conn.query_row(
            "SELECT id, monthly_usd, sponsorship_privacy_level, name, login, thumb_url FROM github_profiles WHERE id = ?1",
            [&github_id],
            |row| Ok(GitHubProfile {
                id: row.get(0)?,
                monthly_usd: row.get::<_, Option<i32>>(1)?.map(|v| v.to_string()),
                sponsorship_privacy_level: row.get(2)?,
                name: row.get(3)?,
                login: row.get(4)?,
                thumb_url: row.get(5)?,
            }),
        ).optional()?
    } else {
        None
    };

    Ok(Some(UserInfo {
        id,
        patreon,
        github,
    }))
}

#[derive(Debug)]
struct CreateUserArgs {
    patreon_user_id: Option<String>,
    github_user_id: Option<String>,
}

fn create_user(pool: &Pool, args: CreateUserArgs) -> eyre::Result<i64> {
    use rand::Rng;

    // Generate a 32-character API key
    let api_key: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO users (patreon_user_id, github_user_id, api_key, last_seen) VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)",
        rusqlite::params![
            args.patreon_user_id,
            args.github_user_id,
            api_key
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

async fn patreon_callback(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    let body = std::str::from_utf8(&body[..])?;
    let args: PatreonCallbackArgs = facet_json::from_str(body)?;

    let mod_patreon = libpatreon::load();
    let pool = &ts.pool;

    let creds = mod_patreon
        .handle_oauth_callback(&ts.ti.tc, global_state().web, &args)
        .await?;

    let res: Option<PatreonCallbackResponse> = match creds {
        Some(creds) => {
            let profile = mod_patreon.fetch_profile(&ts.rc()?, creds).await?;
            save_patreon_profile(&pool, &profile);
            save_patreon_credentials(&pool, &profile.id, &creds)?;

            // now, do we already have a user with this patreon profile? if not, create it
            // TODO: eventually, we should accept a "user_id" to assign this profile too, in
            // case it gets created ahead of time and we're just "linking" a profile to an existing user.
            let conn = pool.get()?;
            let existing_user: Option<i64> = conn
                .query_row(
                    "SELECT id FROM users WHERE patreon_user_id = ?1",
                    [&profile.id],
                    |row| row.get(0),
                )
                .optional()?;

            let user_id = if let Some(user_id) = existing_user {
                user_id
            } else {
                create_user(
                    &pool,
                    CreateUserArgs {
                        patreon_user_id: Some(profile.id.clone()),
                        github_user_id: None,
                    },
                )?
            };

            let user_info = {
                let user_id_str = user_id.to_string();
                fetch_user_info(&pool, &user_id_str)?.unwrap()
            };

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
    let args: GitHubCallbackArgs = facet_json::from_str(body)?;

    let mod_github = libgithub::load();

    let web = global_state().web;
    let creds = mod_github
        .handle_oauth_callback(&ts.ti.tc, web, &args)
        .await?;

    let res: Option<GitHubCallbackResponse> = match creds {
        Some(creds) => {
            let rc = ts.rc()?;
            let (github_creds, site_creds) = mod_github.fetch_profile(&rc, web, creds).await?;

            // Save GitHub credentials to the database
            let conn = ts.pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO github_credentials (github_id, data) VALUES (?1, ?2)",
                rusqlite::params![
                    site_creds.user_info.profile.github_id,
                    facet_json::to_string(&github_creds)
                ],
            )?;
            Some(GitHubCallbackResponse {
                auth_bundle: site_creds,
                github_credentials: github_creds,
            })
        }
        None => None,
    };
    FacetJson(res).into_reply()
}

fn get_patreon_credentials(
    conn: &rusqlite::Connection,
    patreon_id: &str,
) -> Result<PatreonCredentials, HttpError> {
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

    facet_json::from_str::<PatreonCredentials>(&pat_creds_payload).map_err(|_| {
        HttpError::with_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to parse Patreon credentials",
        )
    })
}

fn get_github_credentials(
    conn: &rusqlite::Connection,
    github_id: &str,
) -> Result<GithubCredentials, HttpError> {
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

    facet_json::from_str::<GithubCredentials>(&github_creds).map_err(|_| {
        HttpError::with_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to parse GitHub credentials",
        )
    })
}

// #[axum::debug_handler]
async fn refresh_profile(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    body: Bytes,
) -> Reply {
    todo!(
        "take global user ID, refresh patreon/github/whatever is connected, return updated Profile object (saving it). design response so that we can let the caller know if/when credentials are expired"
    );
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
