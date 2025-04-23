use std::sync::Arc;

use axum::extract::FromRequestParts;
use config_types::TenantDomain;
use libhttpclient::{StatusCode, request::Parts};
use serde::Deserialize;

use crate::impls::{
    MomTenantState, global_state,
    site::{IntoReply as _, Reply},
};

#[derive(Clone)]
pub struct TenantExtractor(pub Arc<MomTenantState>);

impl<S> FromRequestParts<S> for TenantExtractor
where
    S: Send + Sync,
{
    type Rejection = Reply;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        #[derive(Deserialize)]
        struct PathParts {
            tenant_name: TenantDomain,
        }

        let path_parts: PathParts =
            match axum::extract::Path::<PathParts>::from_request_parts(parts, state).await {
                Ok(p) => p.0,
                Err(e) => {
                    tracing::warn!("path should have :tenant_name, but got {e}");
                    return Err((StatusCode::BAD_REQUEST, e.to_string()).into_reply());
                }
            };

        match global_state().tenants.get(&path_parts.tenant_name).cloned() {
            Some(ts) => Ok(TenantExtractor(ts)),
            None => Err((StatusCode::NOT_FOUND, "Tenant not found").into_reply()),
        }
    }
}
