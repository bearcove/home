use std::collections::HashSet;

use credentials::{
    DiscordUserId, DiscordUserIdRef, GithubProfile, GithubUserId, GithubUserIdRef, PatreonProfile,
    PatreonUserId, PatreonUserIdRef, UserApiKey, UserId, UserIdRef, UserInfo,
};
use libdiscord::DiscordCredentials;
use libgithub::GithubCredentials;
use libhttpclient::HttpClient;
use libpatreon::PatreonCredentials;
use mom_types::AllUsers;
use rusqlite::OptionalExtension;
use time::OffsetDateTime;

use crate::impls::{MomTenantState, SqlitePool, global_state};

pub(crate) async fn refresh_sponsors(ts: &MomTenantState) -> eyre::Result<AllUsers> {
    let client = global_state().client.clone();
    let start_time = std::time::Instant::now();

    let (github_result, patreon_result) = futures_util::future::join(
        async {
            let github_start = std::time::Instant::now();
            let result = refresh_github_sponsors(ts, client.as_ref()).await;
            let github_duration = github_start.elapsed();
            log::info!("GitHub sponsors refresh took {github_duration:?}");
            result
        },
        async {
            let patreon_start = std::time::Instant::now();
            let result = refresh_patreon_sponsors(ts, client.as_ref()).await;
            let patreon_duration = patreon_start.elapsed();
            log::info!("Patreon sponsors refresh took {patreon_duration:?}");
            result
        },
    )
    .await;

    if let Err(e) = github_result {
        log::warn!("Failed to refresh GitHub sponsors: {e}");
    }

    if let Err(e) = patreon_result {
        log::warn!("Failed to refresh Patreon sponsors: {e}");
    }

    let total_duration = start_time.elapsed();
    log::info!("Total sponsors refresh took {total_duration:?}");

    fetch_all_users(&ts.pool)
}

async fn refresh_patreon_sponsors(
    ts: &MomTenantState,
    client: &dyn HttpClient,
) -> eyre::Result<()> {
    let patreon = libpatreon::load();
    let rc = ts.rc()?;

    // Get the creator's Patreon ID from the pak
    let creator_patreon_id = {
        let pak = ts.pak.lock();
        pak.as_ref()
            .and_then(|pak| pak.rc.admin_patreon_ids.first().cloned())
            .ok_or_else(|| eyre::eyre!("admin_patreon_ids should have at least one element"))?
    };

    let creds = fetch_uptodate_patreon_credentials(ts, &creator_patreon_id)
        .await?
        .ok_or_else(|| eyre::eyre!("creator needs to log in with Patreon first"))?;

    let profiles = patreon.list_sponsors(&rc, client, &creds).await?;

    // Check which Patreon profiles already exist in the database
    let conn = ts.pool.get()?;
    let mut existing_patreon_ids = HashSet::new();

    if !profiles.is_empty() {
        let placeholders = profiles.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query =
            format!("SELECT patreon_user_id FROM users WHERE patreon_user_id IN ({placeholders})");

        let mut stmt = conn.prepare(&query)?;
        let patreon_ids: Vec<PatreonUserId> = profiles.iter().map(|p| p.id.clone()).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(&patreon_ids), |row| {
            row.get::<_, PatreonUserId>(0)
        })?;

        for row in rows {
            existing_patreon_ids.insert(row?);
        }
    }

    // Create users for Patreon profiles that don't exist, and save all profiles
    for profile in &profiles {
        if !existing_patreon_ids.contains(&profile.id) {
            let user_id = create_user(
                &ts.pool,
                CreateUserArgs {
                    patreon_user_id: Some(profile.id.clone()),
                    github_user_id: None,
                    discord_user_id: None,
                },
            )?;
            log::info!(
                "Created user {} for Patreon profile {}",
                user_id,
                profile.id
            );
        }

        // Save the Patreon profile to the database (for all profiles)
        save_patreon_profile(&ts.pool, profile)?;
    }

    Ok(())
}

async fn refresh_github_sponsors(ts: &MomTenantState, client: &dyn HttpClient) -> eyre::Result<()> {
    let github = libgithub::load();

    let creator_github_id = {
        let pak = ts.pak.lock();
        let pak = pak.as_ref().ok_or_else(|| eyre::eyre!("pak is not set"))?;
        pak.rc
            .admin_github_ids
            .first()
            .cloned()
            .ok_or_else(|| eyre::eyre!("admin_github_ids should have at least one element"))?
    };

    let creds = fetch_uptodate_github_credentials(ts, &creator_github_id)
        .await?
        .ok_or_else(|| eyre::eyre!("creator needs to log in with Github first"))?;
    let profiles = github.list_sponsors(client, &creds).await?;

    // Check which GitHub profiles already exist in the database
    let conn = ts.pool.get()?;
    let mut existing_github_ids = HashSet::new();

    if !profiles.is_empty() {
        let placeholders = profiles.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query =
            format!("SELECT github_user_id FROM users WHERE github_user_id IN ({placeholders})");

        let mut stmt = conn.prepare(&query)?;
        let github_ids: Vec<GithubUserId> = profiles.iter().map(|p| p.id.clone()).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(&github_ids), |row| {
            row.get::<_, GithubUserId>(0)
        })?;

        for row in rows {
            existing_github_ids.insert(row?);
        }
    }

    // Create users for GitHub profiles that don't exist, and save all profiles
    for profile in &profiles {
        if !existing_github_ids.contains(&profile.id) {
            let user_id = create_user(
                &ts.pool,
                CreateUserArgs {
                    patreon_user_id: None,
                    github_user_id: Some(profile.id.clone()),
                    discord_user_id: None,
                },
            )?;
            log::info!("Created user {} for Github profile {}", user_id, profile.id);
        }
        // Save the Github profile to the database (for all profiles)
        save_github_profile(&ts.pool, profile)?;
    }

    Ok(())
}

/// Returns a full [UserInfo] with associated patreon/github/discord profiles
pub(crate) fn fetch_user_info(
    pool: &SqlitePool,
    user_id: &UserIdRef,
) -> eyre::Result<Option<UserInfo>> {
    let conn = pool.get()?;

    let user_row = conn
        .query_row(
            "
            SELECT
                u.id,
                u.patreon_user_id,
                u.github_user_id,
                u.discord_user_id,
                p.id as p_id,
                p.tier as p_tier,
                p.full_name as p_full_name,
                p.avatar_url as p_avatar_url,
                g.id as g_id,
                g.monthly_usd as g_monthly_usd,
                g.sponsorship_privacy_level as g_sponsorship_privacy_level,
                g.name as g_name,
                g.login as g_login,
                g.avatar_url as g_avatar_url,
                d.id as d_id,
                d.username as d_username,
                d.global_name as d_global_name,
                d.avatar_hash as d_avatar_hash
            FROM users u
            LEFT JOIN patreon_profiles p ON u.patreon_user_id = p.id
            LEFT JOIN github_profiles g ON u.github_user_id = g.id
            LEFT JOIN discord_profiles d ON u.discord_user_id = d.id
            WHERE u.id = ?1
            ",
            [user_id],
            |row| {
                let id: UserId = UserId::new(row.get::<_, i64>("id")?.to_string());
                let patreon_user_id: Option<PatreonUserId> = row.get("patreon_user_id")?;
                let github_user_id: Option<GithubUserId> = row.get("github_user_id")?;
                let discord_user_id: Option<DiscordUserId> = row.get("discord_user_id")?;

                // Build Patreon profile if data exists
                let patreon = if patreon_user_id.is_some() {
                    let p_id: Option<PatreonUserId> = row.get("p_id")?;
                    if p_id.is_some() {
                        Some(PatreonProfile {
                            id: row.get("p_id")?,
                            tier: row.get("p_tier")?,
                            full_name: row.get("p_full_name")?,
                            avatar_url: row.get("p_avatar_url")?,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Build Github profile if data exists
                let github = if github_user_id.is_some() {
                    let g_id: Option<GithubUserId> = row.get("g_id")?;
                    if g_id.is_some() {
                        Some(GithubProfile {
                            id: row.get("g_id")?,
                            monthly_usd: row.get("g_monthly_usd")?,
                            sponsorship_privacy_level: row.get("g_sponsorship_privacy_level")?,
                            name: row.get("g_name")?,
                            login: row.get("g_login")?,
                            avatar_url: row.get("g_avatar_url")?,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Build Discord profile if data exists
                let discord = if discord_user_id.is_some() {
                    let d_id: Option<DiscordUserId> = row.get("d_id")?;
                    if d_id.is_some() {
                        Some(credentials::DiscordProfile {
                            id: row.get("d_id")?,
                            username: row.get("d_username")?,
                            global_name: row.get("d_global_name")?,
                            avatar_hash: row.get("d_avatar_hash")?,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(UserInfo {
                    id,
                    fetched_at: OffsetDateTime::now_utc(),
                    patreon,
                    github,
                    discord,
                })
            },
        )
        .optional()?;

    Ok(user_row)
}

#[derive(Debug)]
pub(crate) struct CreateUserArgs {
    pub(crate) patreon_user_id: Option<PatreonUserId>,
    pub(crate) github_user_id: Option<GithubUserId>,
    pub(crate) discord_user_id: Option<DiscordUserId>,
}

pub(crate) fn create_user(pool: &SqlitePool, args: CreateUserArgs) -> eyre::Result<UserId> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO users (patreon_user_id, github_user_id, discord_user_id) VALUES (?1, ?2, ?3)",
        rusqlite::params![
            args.patreon_user_id,
            args.github_user_id,
            args.discord_user_id
        ],
    )?;

    Ok(UserId::new(conn.last_insert_rowid().to_string()))
}

pub(crate) fn create_user_or_attach_profile(
    pool: &SqlitePool,
    args: CreateUserArgs,
    existing_user_id: Option<UserId>,
) -> eyre::Result<UserId> {
    match existing_user_id {
        Some(user_id) => {
            // Attach profile(s) to existing user
            let conn = pool.get()?;

            let mut updates = Vec::new();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(patreon_id) = args.patreon_user_id {
                updates.push("patreon_user_id = ?");
                params.push(Box::new(patreon_id));
            }

            if let Some(github_id) = args.github_user_id {
                updates.push("github_user_id = ?");
                params.push(Box::new(github_id));
            }

            if let Some(discord_id) = args.discord_user_id {
                updates.push("discord_user_id = ?");
                params.push(Box::new(discord_id));
            }

            if !updates.is_empty() {
                let query = format!("UPDATE users SET {} WHERE id = ?", updates.join(", "));
                params.push(Box::new(user_id.to_string()));

                conn.execute(
                    &query,
                    rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
                )?;
            }

            Ok(user_id)
        }
        None => {
            // Create new user
            create_user(pool, args)
        }
    }
}

pub(crate) fn fetch_github_credentials(
    pool: &SqlitePool,
    github_user_id: &GithubUserIdRef,
) -> eyre::Result<Option<GithubCredentials>> {
    let conn = pool.get()?;

    let creds: Option<GithubCredentials> = conn
        .query_row(
            "SELECT access_token, scope, expires_at FROM github_credentials WHERE id = ?1",
            [github_user_id],
            |row| {
                let access_token: String = row.get(0)?;
                let scope: String = row.get(1)?;
                let expires_at: OffsetDateTime = row.get(2)?;

                Ok(GithubCredentials {
                    access_token,
                    scope,
                    expires_at,
                })
            },
        )
        .optional()?;

    Ok(creds)
}

pub(crate) async fn fetch_uptodate_github_credentials(
    ts: &MomTenantState,
    github_user_id: &GithubUserIdRef,
) -> eyre::Result<Option<GithubCredentials>> {
    let creds = fetch_github_credentials(&ts.pool, github_user_id)?;

    let Some(creds) = creds else {
        return Ok(None);
    };

    let client = global_state().client.as_ref();

    if creds.expire_soon() {
        let github = libgithub::load();
        let refreshed_creds = github
            .refresh_credentials(&ts.ti.tc, &creds, client)
            .await?;
        save_github_credentials(&ts.pool, github_user_id, &refreshed_creds)?;
        Ok(Some(refreshed_creds))
    } else {
        Ok(Some(creds))
    }
}

pub(crate) fn save_github_credentials(
    pool: &SqlitePool,
    github_id: &GithubUserIdRef,
    credentials: &GithubCredentials,
) -> eyre::Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "
        INSERT INTO github_credentials (
            id,
            access_token,
            scope,
            expires_at
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(id) DO UPDATE SET
            access_token = excluded.access_token,
            scope = excluded.scope,
            expires_at = excluded.expires_at
        ",
        rusqlite::params![
            github_id,
            credentials.access_token,
            credentials.scope,
            credentials.expires_at
        ],
    )?;
    Ok(())
}

pub(crate) fn save_github_profile(
    pool: &SqlitePool,
    profile: &credentials::GithubProfile,
) -> eyre::Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "
        INSERT INTO github_profiles (
            id,
            monthly_usd,
            sponsorship_privacy_level,
            name,
            login,
            avatar_url,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
            monthly_usd = excluded.monthly_usd,
            sponsorship_privacy_level = excluded.sponsorship_privacy_level,
            name = excluded.name,
            login = excluded.login,
            avatar_url = excluded.avatar_url,
            updated_at = excluded.updated_at
        ",
        rusqlite::params![
            profile.id,
            profile.monthly_usd,
            profile.sponsorship_privacy_level,
            profile.name,
            profile.login,
            profile.avatar_url
        ],
    )?;
    Ok(())
}

pub(crate) fn fetch_patreon_credentials(
    pool: &SqlitePool,
    patreon_user_id: &PatreonUserIdRef,
) -> eyre::Result<Option<PatreonCredentials>> {
    let conn = pool.get()?;

    let creds: Option<PatreonCredentials> = conn
        .query_row(
            "SELECT access_token, refresh_token, expires_at FROM patreon_credentials WHERE id = ?1",
            [patreon_user_id],
            |row| {
                let access_token: String = row.get(0)?;
                let refresh_token: String = row.get(1)?;
                let expires_at: OffsetDateTime = row.get(2)?;

                Ok(PatreonCredentials {
                    access_token,
                    refresh_token,
                    expires_at,
                })
            },
        )
        .optional()?;

    Ok(creds)
}

pub(crate) async fn fetch_uptodate_patreon_credentials(
    ts: &MomTenantState,
    patreon_user_id: &PatreonUserIdRef,
) -> eyre::Result<Option<PatreonCredentials>> {
    let creds = fetch_patreon_credentials(&ts.pool, patreon_user_id)?;
    let Some(creds) = creds else {
        return Ok(None);
    };

    let client = global_state().client.as_ref();

    if creds.expire_soon() {
        let patreon = libpatreon::load();
        let refreshed_creds = patreon
            .refresh_credentials(&ts.ti.tc, &creds, client)
            .await?;
        save_patreon_credentials(&ts.pool, patreon_user_id, &refreshed_creds)?;
        Ok(Some(refreshed_creds))
    } else {
        Ok(Some(creds))
    }
}

pub(crate) fn save_patreon_credentials(
    pool: &SqlitePool,
    patreon_id: &PatreonUserIdRef,
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

pub(crate) fn save_patreon_profile(
    pool: &SqlitePool,
    profile: &PatreonProfile,
) -> eyre::Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT OR REPLACE INTO patreon_profiles (id, tier, full_name, avatar_url, updated_at) VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)",
        rusqlite::params![
            profile.id,
            profile.tier,
            profile.full_name,
            profile.avatar_url
        ],
    )?;
    Ok(())
}

pub(crate) fn fetch_discord_credentials(
    pool: &SqlitePool,
    discord_user_id: &DiscordUserIdRef,
) -> eyre::Result<Option<DiscordCredentials>> {
    let conn = pool.get()?;

    let creds: Option<DiscordCredentials> = conn
        .query_row(
            "SELECT access_token, refresh_token, expires_at FROM discord_credentials WHERE id = ?1",
            [discord_user_id],
            |row| {
                let access_token: String = row.get(0)?;
                let refresh_token: String = row.get(1)?;
                let expires_at: OffsetDateTime = row.get(2)?;

                Ok(DiscordCredentials {
                    access_token,
                    refresh_token,
                    expires_at,
                })
            },
        )
        .optional()?;

    Ok(creds)
}

pub(crate) async fn fetch_uptodate_discord_credentials(
    ts: &MomTenantState,
    discord_user_id: &DiscordUserIdRef,
) -> eyre::Result<Option<DiscordCredentials>> {
    let creds = fetch_discord_credentials(&ts.pool, discord_user_id)?;
    let Some(creds) = creds else {
        return Ok(None);
    };

    let client = global_state().client.as_ref();

    if creds.expire_soon() {
        let discord = libdiscord::load();
        let refreshed_creds = discord
            .refresh_credentials(&ts.ti.tc, &creds, client)
            .await?;
        save_discord_credentials(&ts.pool, discord_user_id, &refreshed_creds)?;
        Ok(Some(refreshed_creds))
    } else {
        Ok(Some(creds))
    }
}

pub(crate) fn save_discord_credentials(
    pool: &SqlitePool,
    discord_id: &DiscordUserIdRef,
    credentials: &DiscordCredentials,
) -> eyre::Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT OR REPLACE INTO discord_credentials (id, access_token, refresh_token, expires_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            discord_id,
            credentials.access_token,
            credentials.refresh_token,
            credentials.expires_at
        ],
    )?;
    Ok(())
}

pub(crate) fn save_discord_profile(
    pool: &SqlitePool,
    profile: &credentials::DiscordProfile,
) -> eyre::Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "
        INSERT INTO discord_profiles (
            id,
            username,
            global_name,
            avatar_hash,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            global_name = excluded.global_name,
            avatar_hash = excluded.avatar_hash,
            updated_at = excluded.updated_at
        ",
        rusqlite::params![
            profile.id,
            profile.username,
            profile.global_name,
            profile.avatar_hash
        ],
    )?;
    Ok(())
}

pub(crate) async fn refresh_userinfo(
    ts: &MomTenantState,
    user_id: &UserIdRef,
) -> eyre::Result<UserInfo> {
    log::info!("Refreshing user info for {user_id}");
    let conn = ts.pool.get()?;

    // First, fetch the user record
    let user_row: Option<(i64, Option<PatreonUserId>, Option<GithubUserId>)> = conn
        .query_row(
            "SELECT id, patreon_user_id, github_user_id FROM users WHERE id = ?1",
            [user_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<PatreonUserId>>(1)?,
                    row.get::<_, Option<GithubUserId>>(2)?,
                ))
            },
        )
        .optional()?;

    let Some((id, patreon_user_id, github_user_id)) = user_row else {
        return Err(eyre::eyre!("User with id {} not found", user_id));
    };
    let id = UserId::new(id.to_string());

    let client = global_state().client.as_ref();
    let rc = ts.rc()?;

    // Refresh Patreon profile if linked
    let patreon = if let Some(patreon_id) = patreon_user_id {
        let creds = fetch_uptodate_patreon_credentials(ts, &patreon_id)
            .await?
            .ok_or_else(|| eyre::eyre!("No Patreon credentials found for user {}", patreon_id))?;

        let patreon = libpatreon::load();
        let profile = patreon.fetch_profile(&rc, &creds, client).await?;
        save_patreon_profile(&ts.pool, &profile)?;

        Some(profile)
    } else {
        None
    };

    // Refresh Github profile if linked
    let github = if let Some(github_id) = github_user_id {
        let creds = fetch_uptodate_github_credentials(ts, &github_id)
            .await?
            .ok_or_else(|| eyre::eyre!("No Github credentials found for user {}", github_id))?;

        let github = libgithub::load();
        let profile = github.fetch_profile(&creds, client).await?;
        save_github_profile(&ts.pool, &profile)?;

        Some(profile)
    } else {
        None
    };

    // Refresh Discord profile if linked
    let discord = {
        let discord_user_id = conn
            .query_row(
                "SELECT discord_user_id FROM users WHERE id = ?1",
                [&id.to_string()],
                |row| row.get::<_, Option<DiscordUserId>>(0),
            )
            .optional()?
            .flatten();

        if let Some(discord_id) = discord_user_id {
            let creds = fetch_uptodate_discord_credentials(ts, &discord_id)
                .await?
                .ok_or_else(|| {
                    eyre::eyre!("No Discord credentials found for user {}", discord_id)
                })?;

            let discord = libdiscord::load();
            let profile = discord.fetch_profile(&creds, client).await?;
            save_discord_profile(&ts.pool, &profile)?;

            Some(profile)
        } else {
            None
        }
    };

    Ok(UserInfo {
        id,
        fetched_at: OffsetDateTime::now_utc(),
        patreon,
        github,
        discord,
    })
}

pub(crate) fn fetch_all_users(pool: &SqlitePool) -> eyre::Result<AllUsers> {
    let start_time = std::time::Instant::now();

    let conn = pool.get()?;

    let mut stmt = conn.prepare(
        "
        SELECT
            u.id,
            u.patreon_user_id,
            u.github_user_id,
            u.discord_user_id,
            p.id as p_id,
            p.tier as p_tier,
            p.full_name as p_full_name,
            p.avatar_url as p_avatar_url,
            g.id as g_id,
            g.monthly_usd as g_monthly_usd,
            g.sponsorship_privacy_level as g_sponsorship_privacy_level,
            g.name as g_name,
            g.login as g_login,
            g.avatar_url as g_avatar_url,
            d.id as d_id,
            d.username as d_username,
            d.global_name as d_global_name,
            d.avatar_hash as d_avatar_hash
        FROM users u
        LEFT JOIN patreon_profiles p ON u.patreon_user_id = p.id
        LEFT JOIN github_profiles g ON u.github_user_id = g.id
        LEFT JOIN discord_profiles d ON u.discord_user_id = d.id
        ",
    )?;

    let rows = stmt.query_map([], |row| {
        let id: UserId = UserId::new(row.get::<_, i64>("id")?.to_string());
        let patreon_user_id: Option<PatreonUserId> = row.get("patreon_user_id")?;
        let github_user_id: Option<GithubUserId> = row.get("github_user_id")?;
        let discord_user_id: Option<DiscordUserId> = row.get("discord_user_id")?;

        // Build Patreon profile if data exists
        let patreon = if patreon_user_id.is_some() {
            let p_id: Option<PatreonUserId> = row.get("p_id")?;
            if p_id.is_some() {
                Some(PatreonProfile {
                    id: row.get("p_id")?,
                    tier: row.get("p_tier")?,
                    full_name: row.get("p_full_name")?,
                    avatar_url: row.get("p_avatar_url")?,
                })
            } else {
                None
            }
        } else {
            None
        };

        // Build Github profile if data exists
        let github = if github_user_id.is_some() {
            let g_id: Option<GithubUserId> = row.get("g_id")?;
            if g_id.is_some() {
                Some(GithubProfile {
                    id: row.get("g_id")?,
                    monthly_usd: row.get("g_monthly_usd")?,
                    sponsorship_privacy_level: row.get("g_sponsorship_privacy_level")?,
                    name: row.get("g_name")?,
                    login: row.get("g_login")?,
                    avatar_url: row.get("g_avatar_url")?,
                })
            } else {
                None
            }
        } else {
            None
        };

        // Build Discord profile if data exists
        let discord = if discord_user_id.is_some() {
            let d_id: Option<DiscordUserId> = row.get("d_id")?;
            if d_id.is_some() {
                Some(credentials::DiscordProfile {
                    id: row.get("d_id")?,
                    username: row.get("d_username")?,
                    global_name: row.get("d_global_name")?,
                    avatar_hash: row.get("d_avatar_hash")?,
                })
            } else {
                None
            }
        } else {
            None
        };

        Ok(UserInfo {
            id,
            fetched_at: OffsetDateTime::now_utc(),
            patreon,
            github,
            discord,
        })
    })?;

    let mut users = Vec::new();
    for row in rows {
        users.push(row?);
    }

    let duration = start_time.elapsed();
    log::info!("fetch_all_users took {duration:?}");

    Ok(AllUsers {
        users: users.into_iter().map(|u| (u.id.clone(), u)).collect(),
    })
}

fn generate_api_key() -> UserApiKey {
    use rand::Rng;

    rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect::<String>()
        .into()
}

pub(crate) fn make_api_key(pool: &SqlitePool, user_id: &UserIdRef) -> eyre::Result<UserApiKey> {
    let conn = pool.get()?;

    // First, check if there's already a non-revoked API key for this user
    let existing_key: Option<UserApiKey> = conn
        .query_row(
            "SELECT id FROM api_keys WHERE user_id = ?1 AND revoked_at IS NULL",
            [&user_id],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(key) = existing_key {
        return Ok(key);
    }

    // No existing key found, generate a new one
    let api_key = generate_api_key();

    conn.execute(
        "INSERT INTO api_keys (id, user_id) VALUES (?1, ?2)",
        rusqlite::params![api_key, user_id],
    )?;

    Ok(api_key)
}

pub(crate) fn verify_api_key(pool: &SqlitePool, api_key: &UserApiKey) -> eyre::Result<UserInfo> {
    let conn = pool.get()?;

    // First, check if the API key exists and is not revoked
    let user_id: Option<i64> = conn
        .query_row(
            "SELECT user_id FROM api_keys WHERE id = ?1 AND revoked_at IS NULL",
            [api_key],
            |row| row.get(0),
        )
        .optional()?;

    let user_id = user_id.map(|id| UserId::new(id.to_string()));

    let Some(user_id) = user_id else {
        // API key doesn't exist or is revoked
        log::warn!("API key verification failed: key doesn't exist or is revoked");
        return Err(eyre::eyre!("API key doesn't exist or is revoked"));
    };

    // Fetch the user info
    let user_info = fetch_user_info(pool, user_id.as_ref())?;

    let Some(user_info) = user_info else {
        // User doesn't exist (shouldn't happen if DB is consistent)
        log::warn!("API key verification failed: user {user_id} doesn't exist (DB inconsistency)");
        return Err(eyre::eyre!(
            "User {user_id} doesn't exist (DB inconsistency)"
        ));
    };

    Ok(user_info)
}
