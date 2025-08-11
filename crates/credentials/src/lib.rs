use facet::Facet;
use plait::plait;
use serde::Serialize;

pub use eyre::{Result, eyre};

plait! {
    with crates {
        serde
        rusqlite
        minijinja
    }

    /// User identifiers (that can log into various sites)
    pub struct UserId => &UserIdRef;

    /// Github user identifiers
    pub struct GithubUserId => &GithubUserIdRef;

    /// Patreon user identifiers
    pub struct PatreonUserId => &PatreonUserIdRef;
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct UserInfo {
    /// tenants-specific user ID
    pub id: UserId,

    pub patreon: Option<PatreonProfile>,
    pub github: Option<GithubProfile>,
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
