use facet::Facet;
use plait::plait;
use serde::Serialize;

pub use eyre::{Result, eyre};
use time::OffsetDateTime;

plait! {
    with crates {
        serde
        rusqlite
        minijinja
    }

    /// User identifiers (that can log into various sites)
    pub struct UserId => &UserIdRef;

    /// User API keys
    pub struct UserApiKey => &UserApiKeyRef;

    /// Github user identifiers (database ID, not login)
    pub struct GithubUserId => &GithubUserIdRef;

    /// Patreon user identifiers
    pub struct PatreonUserId => &PatreonUserIdRef;

    /// Discord user identifiers — those are snowflakes, we store them as string
    pub struct DiscordUserId => &DiscordUserIdRef;

    /// Discord guild identifiers — those are snowflakes, we store them as string
    pub struct DiscordGuildId => &DiscordGuildIdRef;

    /// Discord role identifiers — those are snowflakes, we store them as string
    pub struct DiscordRoleId => &DiscordRoleIdRef;

    /// Discord channel identifiers — those are snowflakes, we store them as string
    pub struct DiscordChannelId => &DiscordChannelIdRef;

    /// Discord message identifiers — those are snowflakes, we store them as string
    pub struct DiscordMessageId => &DiscordMessageIdRef;

    /// Why did we get such a tier?
    pub struct TierCause => &TierCauseRef;
}

/// An auth bundle, stored in a confidential cookie
#[derive(Debug, Clone, Serialize, Facet)]
pub struct AuthBundle {
    pub user_info: UserInfo,
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct Profile {
    pub name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct UserInfo {
    /// tenant-specific user ID
    pub id: UserId,

    /// last timestamp this user info was updated by mom
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,

    /// patreon profile (if any)
    #[facet(default)]
    pub patreon: Option<PatreonProfile>,

    /// github profile (if any)
    #[facet(default)]
    pub github: Option<GithubProfile>,

    /// discord profile (if any)
    #[facet(default)]
    pub discord: Option<DiscordProfile>,

    /// Is that user a member of the Discord server?
    #[facet(default)]
    pub in_discord: bool,

    /// gifted tier (if any)
    #[facet(default)]
    pub gifted_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct GithubProfile {
    /// Github user ID
    pub id: GithubUserId,

    /// Monthly (recurring) dollars sponsorship, if any.
    /// Do tier mapping yourself, later, per-tenant.
    /// One-time donations don't count.
    pub monthly_usd: Option<u64>,

    /// "PRIVATE" or "PUBLIC"
    pub sponsorship_privacy_level: Option<String>,

    /// Full name (e.g. "Amos Wenger")
    pub name: Option<String>,

    /// Login (e.g. "fasterthanlime")
    pub login: String,

    /// Avatar URL
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct PatreonProfile {
    /// Patreon user ID
    pub id: PatreonUserId,

    /// Sponsor tier title if any (Bronze, Silver, Gold)
    pub tier: Option<String>,

    /// Full name (as given by Patreon)
    pub full_name: String,

    /// Avatar URL
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct DiscordProfile {
    /// Discord user ID
    pub id: DiscordUserId,

    /// Discord username
    pub username: String,

    /// Discord global name (display name)
    pub global_name: Option<String>,

    /// Discord avatar hash
    /// Base URL is https://cdn.discordapp.com/
    /// For avatars you want 'avatars/user_id/user_avatar.png'
    pub avatar_hash: Option<String>,
}

/// hardcoded stuff for fasterthanlime

#[derive(Facet, Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum FasterthanlimeTier {
    None = 0,
    Bronze = 1,
    Silver = 2,
    Gold = 3,
}

impl PartialOrd for FasterthanlimeTier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FasterthanlimeTier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

impl UserInfo {
    pub fn get_fasterthanlime_tier(&self) -> Option<(FasterthanlimeTier, TierCause)> {
        // Check Patreon tier
        let patreon_tier = self
            .patreon
            .as_ref()
            .and_then(|p| p.tier.as_deref())
            .map(|tier| match tier {
                "Bronze" => FasterthanlimeTier::Bronze,
                "Silver" => FasterthanlimeTier::Silver,
                "Gold" | "Creator" => FasterthanlimeTier::Gold,
                _ => FasterthanlimeTier::None,
            })
            .unwrap_or(FasterthanlimeTier::None);

        // Check GitHub sponsorship tier
        let github_tier = self
            .github
            .as_ref()
            .and_then(|g| g.monthly_usd)
            .map(|amount| match amount {
                amount if amount >= 50 => FasterthanlimeTier::Gold,
                amount if amount >= 10 => FasterthanlimeTier::Silver,
                amount if amount >= 5 => FasterthanlimeTier::Bronze,
                _ => FasterthanlimeTier::None,
            })
            .unwrap_or(FasterthanlimeTier::None);

        // Check gifted tier
        let gifted_tier = self
            .gifted_tier
            .as_deref()
            .map(|tier| match tier {
                "Bronze" => FasterthanlimeTier::Bronze,
                "Silver" => FasterthanlimeTier::Silver,
                "Gold" | "Creator" => FasterthanlimeTier::Gold,
                _ => FasterthanlimeTier::None,
            })
            .unwrap_or(FasterthanlimeTier::None);

        // Return the highest tier from any platform
        let highest_tier = patreon_tier.max(github_tier).max(gifted_tier);

        match highest_tier {
            FasterthanlimeTier::None => None,
            tier => {
                let cause = if tier == gifted_tier && tier != FasterthanlimeTier::None {
                    TierCause::from("gift")
                } else if tier == patreon_tier && tier != FasterthanlimeTier::None {
                    TierCause::from("patreon")
                } else {
                    TierCause::from("github")
                };
                Some((tier, cause))
            }
        }
    }

    pub fn name(&self) -> String {
        // Try to get full name from GitHub profile
        if let Some(github) = &self.github {
            if let Some(name) = &github.name {
                if !name.trim().is_empty() {
                    return name.clone();
                }
            }
            // Fall back to GitHub login
            return github.login.clone();
        }

        // Try to get full name from Patreon profile
        if let Some(patreon) = &self.patreon {
            if !patreon.full_name.trim().is_empty() {
                return patreon.full_name.clone();
            }
        }

        // Fall back to user ID
        format!("user #{}", self.id)
    }

    pub fn avatar_url(&self) -> Option<String> {
        self.github
            .as_ref()
            .and_then(|g| g.avatar_url.clone())
            .or_else(|| self.patreon.as_ref().and_then(|p| p.avatar_url.clone()))
            .or_else(|| {
                self.discord.as_ref().and_then(|d| {
                    d.avatar_hash
                        .as_ref()
                        .map(|hash| build_discord_avatar_url(&d.id, hash))
                })
            })
    }

    pub fn get_profile(&self) -> Profile {
        Profile {
            name: self.name(),
            avatar_url: self.avatar_url(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.patreon.is_none() && self.github.is_none() && self.discord.is_none()
    }
}

fn build_discord_avatar_url(user_id: &DiscordUserIdRef, avatar_hash: &str) -> String {
    format!("https://cdn.discordapp.com/avatars/{user_id}/{avatar_hash}.png")
}

impl FasterthanlimeTier {
    pub fn has_bronze(self) -> bool {
        self >= FasterthanlimeTier::Bronze
    }

    pub fn has_silver(self) -> bool {
        self >= FasterthanlimeTier::Silver
    }

    pub fn has_gold(self) -> bool {
        self >= FasterthanlimeTier::Gold
    }
}
