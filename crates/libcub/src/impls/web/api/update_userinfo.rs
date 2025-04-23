use conflux::Viewer;
use credentials::UserInfo;
use cub_types::CubTenant;
use http::StatusCode;

use crate::impls::{
    credentials::auth_bundle_as_cookie,
    cub_req::CubReqImpl,
    reply::{IntoLegacyReply, LegacyHttpError, LegacyReply, MerdeJson},
};

/// The userinfo after updating it
struct UpdatedUserInfo<'s> {
    viewer: Viewer,
    user_info: UserInfo<'s>,
}

merde::derive!(
    impl (Serialize, Deserialize) for struct UpdatedUserInfo<'s> { viewer, user_info }
);

/// Does another GitHub/Patreon API call to re-check someone's tier.
pub(crate) async fn serve_update_userinfo(mut tr: CubReqImpl) -> LegacyReply {
    let auth_bundle = match tr.auth_bundle.as_ref() {
        Some(creds) => creds,
        None => {
            return LegacyHttpError::with_status(StatusCode::UNAUTHORIZED, "Not logged in")
                .into_legacy_reply();
        }
    };

    let tcli = tr.tenant.tcli();
    let new_auth_bundle = tcli.update_auth_bundle(auth_bundle).await?;
    tracing::info!("New auth bundle: {new_auth_bundle:#?}");
    tr.auth_bundle = Some(new_auth_bundle.clone());
    let viewer = tr.viewer()?;

    tr.cookies.add(auth_bundle_as_cookie(&new_auth_bundle));

    MerdeJson(UpdatedUserInfo {
        viewer,
        user_info: new_auth_bundle.user_info,
    })
    .into_legacy_reply()
}
