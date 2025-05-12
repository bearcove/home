use serde::Serialize;
use time::OffsetDateTime;

pub use eyre::{Result, eyre};

#[derive(Debug, Clone)]
pub struct AuthBundle {
    pub user_info: UserInfo,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
    pub profile: Profile,
    pub tier: Option<Tier>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tier {
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub patreon_id: Option<String>,
    pub github_id: Option<String>,

    // for GitHub that's `name ?? login`
    pub full_name: String,

    // avatar thumbnail URL
    pub thumb_url: String,
}

impl Profile {
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
