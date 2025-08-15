use axum::Extension;
use credentials::UserId;
use facet::Facet;
use mom_types::AllUsers;

use crate::impls::{
    endpoints::tenant_extractor::TenantExtractor,
    site::{FacetJson, IntoReply, Reply},
};

#[allow(dead_code)]
#[derive(Facet)]
#[repr(u8)]
pub enum OpendoorRequest {
    ListAllUsers {},
    SetGiftedTier {
        user_id: UserId,
        tier: Option<String>,
    },
}

#[allow(dead_code)]
#[derive(Facet)]
#[repr(u8)]
pub enum OpendoorResponse {
    ListAllUsers { all_users: AllUsers },
    SetGiftedTier {},
}

pub(crate) async fn opendoor(
    Extension(TenantExtractor(ts)): Extension<TenantExtractor>,
    FacetJson(req): FacetJson<OpendoorRequest>,
) -> Reply {
    match req {
        OpendoorRequest::ListAllUsers {} => {
            let guard = ts.users.lock();
            let all_users = guard.as_ref().clone();
            FacetJson(OpendoorResponse::ListAllUsers { all_users }).into_reply()
        }
        OpendoorRequest::SetGiftedTier { user_id, tier } => {
            let conn = ts.pool.get()?;
            let query = "UPDATE users SET gifted_tier = ? WHERE id = ?";
            let tier_param = tier.as_deref();

            match conn.execute(query, rusqlite::params![tier_param, user_id]) {
                Ok(rows_affected) => {
                    if rows_affected == 0 {
                        return Err(eyre::eyre!("User not found").into());
                    }
                    FacetJson(OpendoorResponse::SetGiftedTier {}).into_reply()
                }
                Err(e) => {
                    log::error!("Failed to update gifted tier: {e}");
                    Err(e.into())
                }
            }
        }
    }
}
