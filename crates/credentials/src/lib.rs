use facet::Facet;
use serde::Serialize;
use time::OffsetDateTime;

pub use eyre::{Result, eyre};

#[derive(Debug, Clone, Facet)]
pub struct AuthBundle {
    pub user_info: UserInfo,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct UserInfo {
    pub profile: Profile,
    pub tier: Option<Tier>,
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct Tier {
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Facet)]
pub struct Profile {
    pub patreon_id: Option<String>,
    pub github_id: Option<String>,
    
    // Email address for email-based authentication
    pub email: Option<String>,

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
        if let Some(email) = &self.email {
            return Ok(format!("email:{email}"));
        }
        Err(eyre!("no global id"))
    }
}
