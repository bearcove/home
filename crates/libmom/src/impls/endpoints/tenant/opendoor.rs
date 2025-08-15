use axum::Extension;
use credentials::{UserId, UserInfo};
use facet::Facet;
use mom_types::AllUsers;

use crate::impls::{
    endpoints::tenant_extractor::TenantExtractor,
    site::{FacetJson, IntoReply, Reply},
    users::refresh_userinfo,
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
#[allow(clippy::large_enum_variant)]
#[derive(Facet)]
#[repr(u8)]
pub enum OpendoorResponse {
    ListAllUsers { all_users: AllUsers },
    SetGiftedTier { user_info: UserInfo },
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
            let query = "UPDATE users SET gifted_tier = ? WHERE id = ?";
            let tier_param = tier.as_deref();

            let query_result = {
                let conn = ts.pool.get()?;
                conn.execute(query, rusqlite::params![tier_param, user_id])
            };

            match query_result {
                Ok(rows_affected) => {
                    if rows_affected == 0 {
                        return Err(eyre::eyre!("User not found").into());
                    }

                    // Refresh user info after updating gifted tier
                    let user_info = match refresh_userinfo(&ts, &user_id).await {
                        Ok(user_info) => user_info,
                        Err(e) => {
                            log::error!(
                                "Failed to refresh user info after updating gifted tier: {e}"
                            );
                            return Err(e.into());
                        }
                    };

                    FacetJson(OpendoorResponse::SetGiftedTier { user_info }).into_reply()
                }
                Err(e) => {
                    log::error!("Failed to update gifted tier: {e}");
                    Err(e.into())
                }
            }
        }
    }
}
