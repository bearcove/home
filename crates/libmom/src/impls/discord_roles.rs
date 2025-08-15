use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use config_types::TenantDomain;
use credentials::{
    DiscordChannelId, DiscordRoleId, DiscordUserId, FasterthanlimeTier, UserId, UserInfo,
};
use eyre::Result;
use libdiscord::DiscordGuild;
use mom_types::AllUsers;

use crate::impls::MomTenantState;

/// Gathered when starting up a discord role synchronization
#[derive(Clone)]
struct DiscordRolesContext {
    guild: DiscordGuild,
    tier_role_map: HashMap<FasterthanlimeTier, DiscordRoleId>,

    /// maps channel names to their discord channel IDs
    channel_ids: HashMap<String, DiscordChannelId>,
}

enum RoleChange {
    Add,
    Remove,
}

// Cache for Discord roles context, keyed by tenant name
static DISCORD_ROLES_CACHE: std::sync::LazyLock<
    Arc<Mutex<HashMap<TenantDomain, DiscordRolesContext>>>,
> = std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

async fn gather_discord_roles_context(ts: &MomTenantState) -> Result<DiscordRolesContext> {
    let cache_key = ts.ti.tc.name.clone();

    // Check cache first
    {
        let cache = DISCORD_ROLES_CACHE.lock().unwrap();
        if let Some(cached_context) = cache.get(&cache_key) {
            log::debug!("Using cached Discord roles context for tenant: {cache_key}");
            return Ok(cached_context.clone());
        }
    }

    log::debug!("Fetching fresh Discord roles context for tenant: {cache_key}");
    let discord_mod = libdiscord::load();

    // Fetch first guild the bot is in
    let guilds = discord_mod.list_bot_guilds(&ts.ti.tc).await?;
    if guilds.is_empty() {
        return Err(eyre::eyre!("Bot is not in any guilds!"));
    }
    let guild = guilds.into_iter().next().unwrap();
    log::info!("Using guild: {} ({})", guild.name, guild.id);

    // Fetch all roles for this server
    let roles = discord_mod.list_guild_roles(&guild.id, &ts.ti.tc).await?;

    // Create mapping between Discord role IDs and FasterthanlimeTier
    let mut tier_role_map: HashMap<FasterthanlimeTier, DiscordRoleId> = HashMap::new();

    for role in &roles {
        let tier = match role.name.as_str() {
            "Bronze" => Some(FasterthanlimeTier::Bronze),
            "Silver" => Some(FasterthanlimeTier::Silver),
            "Gold" => Some(FasterthanlimeTier::Gold),
            _ => None,
        };

        if let Some(tier) = tier {
            tier_role_map.insert(tier, role.id.clone());
            log::info!("Mapped role {} ({}) to tier {:?}", role.name, role.id, tier);
        }
    }

    if tier_role_map.is_empty() {
        return Err(eyre::eyre!("No tier roles found in guild!"));
    }

    // Fetch all channels and build a map from name to ID
    let channels = discord_mod
        .list_guild_channels(&guild.id, &ts.ti.tc)
        .await?;

    let mut channel_ids = HashMap::new();
    for channel in &channels {
        channel_ids.insert(channel.name.clone(), channel.id.clone());
    }

    if !channel_ids.contains_key("bots") {
        log::warn!("No #bots channel found in guild!");
    }

    if !channel_ids.contains_key("lobby") {
        log::warn!("No #lobby channel found in guild!");
    }

    let context = DiscordRolesContext {
        guild,
        tier_role_map,
        channel_ids,
    };

    // Cache the result
    {
        let mut cache = DISCORD_ROLES_CACHE.lock().unwrap();
        cache.insert(cache_key, context.clone());
    }

    Ok(context)
}

impl DiscordRolesContext {
    async fn log(&self, ts: &MomTenantState, channel_name: &str, message: &str) -> Result<()> {
        if let Some(channel_id) = self.channel_ids.get(channel_name) {
            let discord_mod = libdiscord::load();
            discord_mod
                .post_message_to_channel(channel_id, message, &ts.ti.tc)
                .await?;
        } else {
            log::warn!("Channel '{channel_name}' does not exist in guild");
        }
        Ok(())
    }
}

async fn process_single_member(
    member: &libdiscord::DiscordGuildMember,
    expected_tier: Option<FasterthanlimeTier>,
    cx: &DiscordRolesContext,
    ts: &MomTenantState,
) -> Result<usize> {
    let Some(user) = &member.user else {
        return Ok(0);
    };

    let discord_mod = libdiscord::load();

    // Build a map of what tier roles they currently have
    let mut current_tier_roles: HashMap<FasterthanlimeTier, bool> = HashMap::new();
    for (tier, role_id) in &cx.tier_role_map {
        current_tier_roles.insert(*tier, member.roles.contains(role_id));
    }

    // Determine what roles they should have (only one tier at a time)
    let mut expected_tier_roles: HashMap<FasterthanlimeTier, bool> = HashMap::new();
    expected_tier_roles.insert(
        FasterthanlimeTier::Bronze,
        expected_tier == Some(FasterthanlimeTier::Bronze),
    );
    expected_tier_roles.insert(
        FasterthanlimeTier::Silver,
        expected_tier == Some(FasterthanlimeTier::Silver),
    );
    expected_tier_roles.insert(
        FasterthanlimeTier::Gold,
        expected_tier == Some(FasterthanlimeTier::Gold),
    );

    // Build list of actions to take
    let mut actions: Vec<String> = Vec::new();
    let mut role_changes: Vec<(DiscordRoleId, RoleChange)> = Vec::new();

    for (tier, should_have) in &expected_tier_roles {
        let currently_has = current_tier_roles.get(tier).copied().unwrap_or(false);

        if *should_have && !currently_has {
            // Need to add this role
            if let Some(role_id) = cx.tier_role_map.get(tier) {
                actions.push(format!("Adding {tier:?}"));
                role_changes.push((role_id.clone(), RoleChange::Add));
            }
        } else if !should_have && currently_has {
            // Need to remove this role
            if let Some(role_id) = cx.tier_role_map.get(tier) {
                actions.push(format!("Removing {tier:?}"));
                role_changes.push((role_id.clone(), RoleChange::Remove));
            }
        }
    }

    // Check if we're adding a role to send a thank you message to #lobby
    let added_roles: Vec<&FasterthanlimeTier> = role_changes
        .iter()
        .filter_map(|(role_id, role_change)| {
            if matches!(role_change, RoleChange::Add) {
                cx.tier_role_map
                    .iter()
                    .find(|(_, id)| *id == role_id)
                    .map(|(tier, _)| tier)
            } else {
                None
            }
        })
        .collect();

    if !added_roles.is_empty() {
        let lobby_message = {
            use rand::prelude::*;

            let thank_you_messages = [
                "Joining the ROLE tier today: USER!",
                "USER has joined the ROLE tier.",
                "USER enjoy the ROLE tier perks!",
                "USER is now ROLE tier — thanks for your support!",
                "Welcome to the ROLE tier, USER",
            ];

            let mut rng = rand::rng();
            let chosen_message = thank_you_messages.choose(&mut rng).unwrap();

            let role_name = format!("{:?}", added_roles[0]);
            let user_mention = format!("<@{}>", user.id);

            chosen_message
                .replace("USER", &user_mention)
                .replace("ROLE", &role_name)
        };

        cx.log(ts, "lobby", &lobby_message).await?;
    }

    // If there are changes to make, announce them and execute
    if !actions.is_empty() {
        let display_name = user.global_name.as_deref().unwrap_or(&user.username);
        let action_list = actions.join(", ");
        let message = format!("For <@{}> ({}): {}", user.id, display_name, action_list);

        // Send message to #bots channel if it exists
        cx.log(ts, "bots", &message).await?;
        log::info!("{message}");

        // Execute the role changes
        for (role_id, role_change) in role_changes {
            let result = match role_change {
                RoleChange::Add => {
                    discord_mod
                        .add_guild_member_role(
                            &cx.guild.id,
                            user.id.as_str().into(),
                            &role_id,
                            &ts.ti.tc,
                        )
                        .await
                }
                RoleChange::Remove => {
                    discord_mod
                        .remove_guild_member_role(
                            &cx.guild.id,
                            user.id.as_str().into(),
                            &role_id,
                            &ts.ti.tc,
                        )
                        .await
                }
            };

            if let Err(e) = result {
                let error_msg = format!(
                    "Failed to {} role for @{}: {}",
                    match role_change {
                        RoleChange::Add => "add",
                        RoleChange::Remove => "remove",
                    },
                    user.username,
                    e
                );
                log::error!("{error_msg}");

                // Post error to #bots channel
                cx.log(ts, "bots", &error_msg).await?;
            }
        }

        Ok(actions.len())
    } else {
        Ok(0)
    }
}

pub(crate) async fn synchronize_one_discord_role(
    ts: &MomTenantState,
    user_info: &UserInfo,
) -> Result<()> {
    let discord_mod = libdiscord::load();

    // Check if user has Discord profile
    let Some(discord_profile) = &user_info.discord else {
        log::info!("User {} has no Discord profile", user_info.id);
        return Ok(());
    };

    // Gather Discord context
    let cx = gather_discord_roles_context(ts).await?;

    // Get expected tier for this user
    let expected_tier = user_info
        .get_fasterthanlime_tier()
        .map(|(tier, _cause)| tier);

    // Try to fetch the specific guild member
    let member = match discord_mod
        .get_guild_member(&cx.guild.id, &discord_profile.id, &ts.ti.tc)
        .await
    {
        Ok(member) => {
            // User is a member of the guild, upsert them in the database
            let conn = ts.pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO discord_guild_members (guild_id, user_id) VALUES (?1, ?2)",
                [cx.guild.id.as_str(), discord_profile.id.as_str()],
            )?;
            log::info!("User {} is a member of guild {}", user_info.id, cx.guild.id);
            member
        }
        Err(e) => {
            // User is not a member of the guild, remove them from the database if they exist
            let conn = ts.pool.get()?;
            conn.execute(
                "DELETE FROM discord_guild_members WHERE guild_id = ?1 AND user_id = ?2",
                [cx.guild.id.as_str(), discord_profile.id.as_str()],
            )?;
            log::info!(
                "User {} is not a member of guild {}: {}",
                user_info.id,
                cx.guild.id,
                e
            );
            return Ok(());
        }
    };

    // Process this single member
    log::info!(
        "Processing user {} with expected tier: {:?}",
        user_info.id,
        expected_tier
    );
    let changes = process_single_member(&member, expected_tier, &cx, ts).await?;

    if changes > 0 {
        log::info!("Made {} role changes for user {}", changes, user_info.id);
    } else {
        log::info!("No role changes needed for user {}", user_info.id);
    }

    Ok(())
}

pub(crate) async fn synchronize_all_discord_roles(
    ts: &MomTenantState,
    users: &AllUsers,
) -> Result<()> {
    let start_time = Instant::now();
    let discord_mod = libdiscord::load();

    // Gather Discord context
    let cx = gather_discord_roles_context(ts).await?;

    // Build a map from Discord user ID to their expected tier
    let mut discord_tier_map: HashMap<DiscordUserId, FasterthanlimeTier> = HashMap::new();
    for user_info in users.users.values() {
        if let Some(discord_profile) = &user_info.discord {
            if let Some((tier, _cause)) = user_info.get_fasterthanlime_tier() {
                discord_tier_map.insert(discord_profile.id.clone(), tier);
            }
        }
    }

    log::info!(
        "Built Discord tier map with {} entries",
        discord_tier_map.len()
    );

    // Build a map from UserId to DiscordUserId
    let mut user_to_discord_map: HashMap<UserId, DiscordUserId> = HashMap::new();
    for user_info in users.users.values() {
        if let Some(discord_profile) = &user_info.discord {
            user_to_discord_map.insert(user_info.id.clone(), discord_profile.id.clone());
        }
    }

    log::info!(
        "Built user to Discord map with {} entries",
        user_to_discord_map.len()
    );

    // Fetch all members of the server
    let members = discord_mod
        .list_guild_members(&cx.guild.id, &ts.ti.tc)
        .await?;
    log::info!("Fetched {} guild members", members.len());

    // Store guild information in database
    {
        let conn = ts.pool.get()?;
        let guild = &cx.guild;

        conn.execute(
                "INSERT OR REPLACE INTO discord_guilds (guild_id, approximate_member_count, approximate_presence_count) VALUES (?1, ?2, ?3)",
                [
                    guild.id.as_str(),
                    guild.approximate_member_count.map(|c| c.to_string()).as_deref().unwrap_or(""),
                    guild.approximate_presence_count.map(|c| c.to_string()).as_deref().unwrap_or(""),
                ],
            )?;

        // Collect current member user IDs
        let current_member_ids: std::collections::HashSet<String> = members
            .iter()
            .filter_map(|member| {
                member
                    .user
                    .as_ref()
                    .map(|user| user.id.as_str().to_string())
            })
            .collect();

        // Upsert current members
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO discord_guild_members (guild_id, user_id) VALUES (?1, ?2)",
        )?;

        for member in &members {
            if let Some(user) = &member.user {
                stmt.execute([guild.id.as_str(), user.id.as_str()])?;
            }
        }

        // Delete members that are no longer in the guild
        if !current_member_ids.is_empty() {
            let placeholders = current_member_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let delete_query = format!(
                "DELETE FROM discord_guild_members WHERE guild_id = ?1 AND user_id NOT IN ({placeholders})"
            );

            let mut params: Vec<String> = vec![guild.id.as_str().to_string()];
            params.extend(current_member_ids.iter().cloned());

            conn.execute(&delete_query, rusqlite::params_from_iter(params))?;
        }

        log::info!(
            "Updated database with guild {} and {} members",
            guild.id,
            members.len()
        );
    }

    // Track changes made
    let mut total_changes = 0;
    let mut total_users_changed = 0;
    let mut ignored_members = 0;

    // Process each member
    for member in &members {
        // Look up the expected tier for this member
        let expected_tier = member
            .user
            .as_ref()
            .and_then(|user| discord_tier_map.get(&user.id).copied());

        // Only process members that have a corresponding user in our system
        let should_process = member
            .user
            .as_ref()
            .map(|user| {
                // if we don't have a discord user ID, that means they haven't linked their discord account
                // to home and thus maybe their role is assigned by Patreon directly.
                user_to_discord_map
                    .values()
                    .any(|discord_id| discord_id == &user.id)
            })
            .unwrap_or(false);

        if should_process {
            let changes = process_single_member(member, expected_tier, &cx, ts).await?;
            if changes > 0 {
                total_users_changed += 1;
                total_changes += changes;
            }
        } else {
            ignored_members += 1;
        }
    }

    // Send summary message if we made any changes
    if total_changes > 0 {
        let duration = start_time.elapsed();
        let summary = format!(
            "Discord role sync complete: Made {total_changes} role changes for {total_users_changed} users in {duration:.2?} ({ignored_members} members ignored for not having corresponding Discord user ID)"
        );

        log::info!("{summary}");
        cx.log(ts, "bots", &summary).await?;
    } else {
        log::info!(
            "Discord role sync complete: No changes needed (checked {} members, {} ignored for not having corresponding Discord user ID, in {:.2?})",
            members.len() - ignored_members,
            ignored_members,
            start_time.elapsed()
        );
    }

    Ok(())
}
