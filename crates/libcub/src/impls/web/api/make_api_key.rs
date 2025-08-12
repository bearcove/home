use conflux::{AccessOverride, Viewer};
use credentials::UserInfo;
use cub_types::CubTenant;
use facet::Facet;
use http::StatusCode;
use mom_types::MakeApiKeyArgs;

use crate::impls::{
    cub_req::CubReqImpl,
    reply::{FacetJson, IntoLegacyReply, LegacyHttpError, LegacyReply},
};

/// The userinfo after updating it
#[derive(Facet)]
struct UpdatedUserInfo {
    viewer: Viewer,
    user_info: UserInfo,
}

/// Response for API key creation
#[derive(Facet)]
struct ApiKeyResponse {
    api_key: credentials::UserApiKey,
}

/// Creates an API key for the authenticated user
pub(crate) async fn serve_make_api_key(tr: CubReqImpl) -> LegacyReply {
    let auth_bundle = match tr.auth_bundle.as_ref() {
        Some(creds) => creds,
        None => {
            return LegacyHttpError::with_status(StatusCode::UNAUTHORIZED, "Not logged in")
                .into_legacy_reply();
        }
    };

    let rc = tr.tenant.rc()?;
    let access_override = AccessOverride::from_raw_query(tr.raw_query());
    let viewer = Viewer::new(rc, Some(&auth_bundle.user_info), access_override);

    if !viewer.has_bronze {
        return LegacyHttpError::with_status(StatusCode::FORBIDDEN, "Insufficient tier access")
            .into_legacy_reply();
    }

    let tcli = tr.tenant.tcli();
    let response = tcli
        .make_api_key(&MakeApiKeyArgs {
            user_id: auth_bundle.user_info.id.clone(),
        })
        .await?;

    FacetJson(ApiKeyResponse {
        api_key: response.api_key,
    })
    .into_legacy_reply()
}
