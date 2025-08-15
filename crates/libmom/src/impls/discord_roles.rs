use std::collections::HashMap;
use std::time::Instant;

use credentials::{
    DiscordChannelId, DiscordGuildId, DiscordRoleId, DiscordUserId, FasterthanlimeTier, UserInfo,
};
use eyre::Result;
use mom_types::AllUsers;

use crate::impls::MomTenantState;

/// Gathered when starting up a discord role synchronization
struct DiscordRolesContext {
    guild_id: DiscordGuildId,
    tier_role_map: HashMap<FasterthanlimeTier, DiscordRoleId>,
    bots_channel_id: Option<DiscordChannelId>,
}

enum RoleChange {
    Add,
    Remove,
}

async fn gather_discord_roles_context(ts: &MomTenantState) -> Result<DiscordRolesContext> {
    let discord_mod = libdiscord::load();

    // Fetch first guild the bot is in
    let guilds = discord_mod.list_bot_guilds(&ts.ti.tc).await?;
    if guilds.is_empty() {
        return Err(eyre::eyre!("Bot is not in any guilds!"));
    }
    let guild = &guilds[0];
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

    // Fetch all channels and find #bots
    let channels = discord_mod
        .list_guild_channels(&guild.id, &ts.ti.tc)
        .await?;
    let bots_channel = channels.iter().find(|c| c.name == "bots");
    let bots_channel_id = bots_channel.map(|c| c.id.clone());

    if bots_channel_id.is_none() {
        log::warn!("No #bots channel found in guild!");
    }

    Ok(DiscordRolesContext {
        guild_id: guild.id.clone(),
        tier_role_map,
        bots_channel_id,
    })
}

impl DiscordRolesContext {
    async fn log(&self, message: &str, ts: &MomTenantState) -> Result<()> {
        if let Some(channel_id) = &self.bots_channel_id {
            let discord_mod = libdiscord::load();
            discord_mod
                .post_message_to_channel(channel_id, message, &ts.ti.tc)
                .await?;
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

    // If there are changes to make, announce them and execute
    if !actions.is_empty() {
        let display_name = user.global_name.as_deref().unwrap_or(&user.username);
        let action_list = actions.join(", ");
        let message = format!("For <@{}> ({}): {}", user.id, display_name, action_list);

        // Send message to #bots channel if it exists
        cx.log(&message, ts).await?;
        log::info!("{message}");

        // Execute the role changes
        for (role_id, role_change) in role_changes {
            let result = match role_change {
                RoleChange::Add => {
                    discord_mod
                        .add_guild_member_role(
                            &cx.guild_id,
                            user.id.as_str().into(),
                            &role_id,
                            &ts.ti.tc,
                        )
                        .await
                }
                RoleChange::Remove => {
                    discord_mod
                        .remove_guild_member_role(
                            &cx.guild_id,
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
                cx.log(&error_msg, ts).await?;
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
        log::debug!("User {} has no Discord profile", user_info.id);
        return Ok(());
    };

    // Gather Discord context
    let cx = gather_discord_roles_context(ts).await?;

    // Get expected tier for this user
    let expected_tier = user_info
        .get_fasterthanlime_tier()
        .map(|(tier, _cause)| tier);

    // Fetch the specific guild member
    let member = discord_mod
        .get_guild_member(&cx.guild_id, &discord_profile.id, &ts.ti.tc)
        .await?;

    // Process this single member
    let changes = process_single_member(&member, expected_tier, &cx, ts).await?;

    if changes > 0 {
        log::info!("Made {} role changes for user {}", changes, user_info.id);
    } else {
        log::debug!("No role changes needed for user {}", user_info.id);
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

    // Fetch all members of the server
    let members = discord_mod
        .list_guild_members(&cx.guild_id, &ts.ti.tc)
        .await?;
    log::info!("Fetched {} guild members", members.len());

    // Track changes made
    let mut total_changes = 0;
    let mut total_users_changed = 0;

    // Process each member
    for member in &members {
        // Look up the expected tier for this member
        let expected_tier = member
            .user
            .as_ref()
            .and_then(|user| discord_tier_map.get(&user.id).copied());

        let changes = process_single_member(member, expected_tier, &cx, ts).await?;
        if changes > 0 {
            total_users_changed += 1;
            total_changes += changes;
        }
    }

    // Send summary message if we made any changes
    if total_changes > 0 {
        let duration = start_time.elapsed();
        let summary = format!(
            "Discord role sync complete: Made {total_changes} role changes for {total_users_changed} users in {duration:.2?}"
        );

        log::info!("{summary}");

        cx.log(&summary, ts).await?;
    } else {
        log::info!(
            "Discord role sync complete: No changes needed (checked {} members in {:.2?})",
            members.len(),
            start_time.elapsed()
        );
    }

    Ok(())
}
