use merde::{CowStr, time::Rfc3339};
use serde::Serialize;
use time::OffsetDateTime;

pub use eyre::{Result, eyre};

#[derive(Debug, Clone)]
pub struct AuthBundle<'s> {
    pub user_info: UserInfo<'s>,
    pub expires_at: Rfc3339<OffsetDateTime>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct AuthBundle<'s> { user_info, expires_at }
}

#[derive(Debug, Clone, Serialize)]
pub struct UserInfo<'s> {
    pub profile: Profile<'s>,
    pub tier: Option<Tier<'s>>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct UserInfo<'s> { profile, tier }
}

#[derive(Debug, Clone, Serialize)]
pub struct Tier<'s> {
    pub title: CowStr<'s>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Tier<'s> { title }
}

#[derive(Debug, Clone, Serialize)]
pub struct Profile<'s> {
    pub patreon_id: Option<CowStr<'s>>,
    pub github_id: Option<CowStr<'s>>,

    // for GitHub that's `name ?? login`
    pub full_name: CowStr<'s>,

    // avatar thumbnail URL
    pub thumb_url: CowStr<'s>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Profile<'s> { patreon_id, github_id, full_name, thumb_url }
}

impl Profile<'_> {
    pub fn patreon_id(&self) -> Result<&str> {
        self.patreon_id
            .as_deref()
            .ok_or_else(|| eyre!("no patreon id"))
    }

    pub fn github_id(&self) -> Result<&str> {
        self.github_id
            .as_deref()
            .ok_or_else(|| eyre!("no github id"))
    }

    pub fn global_id(&self) -> Result<String> {
        if let Some(id) = &self.patreon_id {
            return Ok(format!("patreon:{id}"));
        }
        if let Some(id) = &self.github_id {
            return Ok(format!("github:{id}"));
        }
        Err(eyre!("no global id"))
    }
}
