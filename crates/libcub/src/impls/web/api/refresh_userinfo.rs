use conflux::Viewer;
use credentials::{Profile, UserInfo};
use cub_types::CubTenant;
use facet::Facet;
use http::StatusCode;
use mom_types::RefreshProfileArgs;

use crate::impls::{
    credentials::auth_bundle_as_cookie,
    cub_req::CubReqImpl,
    reply::{FacetJson, IntoLegacyReply, LegacyHttpError, LegacyReply},
};

#[derive(Facet)]
struct RefreshUserInfoResponse {
    viewer: Viewer,
    profile: Profile,
    user_info: UserInfo,
}

/// Does another Github/Patreon API call to re-check someone's tier.
pub(crate) async fn serve_refresh_userinfo(mut tr: CubReqImpl) -> LegacyReply {
    let auth_bundle = match tr.auth_bundle.as_ref() {
        Some(creds) => creds,
        None => {
            return LegacyHttpError::with_status(StatusCode::UNAUTHORIZED, "Not logged in")
                .into_legacy_reply();
        }
    };

    let tcli = tr.tenant.tcli();
    let userinfo = tcli
        .refresh_userinfo(&RefreshProfileArgs {
            user_id: auth_bundle.user_info.id.clone(),
        })
        .await?;
    log::info!("New userinfo: {userinfo:#?}");
    let ab = credentials::AuthBundle {
        user_info: userinfo.clone(),
    };

    tr.cookies().add(auth_bundle_as_cookie(&ab));
    tr.auth_bundle = Some(ab.clone());

    let viewer = conflux::Viewer::new(
        tr.tenant.rc().map_err(|e| {
            log::error!("Failed to get tenant rc: {e}");
            LegacyHttpError::with_status(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
        })?,
        Some(&ab.user_info),
        conflux::AccessOverride::from_raw_query(tr.raw_query()),
    );

    FacetJson(RefreshUserInfoResponse {
        viewer,
        profile: ab.user_info.get_profile(),
        user_info: ab.user_info,
    })
    .into_legacy_reply()
}
