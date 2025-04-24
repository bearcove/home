use config_types::is_development;
use conflux::{InputPath, PathMappings};
use eyre::Context as _;
use http::StatusCode;
use tracing::warn;

use crate::impls::{
    cub_req::CubReqImpl,
    reply::{IntoLegacyReply, LegacyHttpError, LegacyReply},
};

/// Params for editing an asset file, like a diagram source
struct EditAssetParams {
    // Path to the input asset file to edit
    input_path: InputPath,
}

merde::derive! {
    impl (Deserialize) for struct EditAssetParams { input_path }
}

/// Opens an asset file in the default editor for editing
pub(crate) async fn serve_edit_asset(tr: CubReqImpl, body: axum::body::Bytes) -> LegacyReply {
    if !is_development() {
        return LegacyHttpError::with_status(
            StatusCode::BAD_REQUEST,
            "Edit asset is only available in development",
        )
        .into_legacy_reply();
    }

    let params: EditAssetParams = merde::json::from_str(
        std::str::from_utf8(&body[..]).wrap_err("deserializing body of /edit-asset")?,
    )?;

    let disk_path =
        PathMappings::from_ti(tr.tenant.ti.as_ref()).to_disk_path(&params.input_path)?;
    tokio::spawn(async move {
        let status = tokio::process::Command::new("open")
            .arg(&disk_path)
            .status()
            .await;

        if let Err(e) = status {
            warn!("Failed to open file: {}", e);
        }
    });

    "OK".into_legacy_reply()
}
