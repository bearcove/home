use crate::impls::{
    cub_req::CubReqImpl,
    reply::{IntoLegacyReply, LegacyHttpError, LegacyReply},
};
use config_types::is_development;
use conflux::{InputPath, PathMappings};
use cub_types::CubReq;
use eyre::Context as _;
use http::StatusCode;
use tracing::warn;

/// Params for opening a file in editor based on its input path
/// and possibly a byte offset or line number to position the cursor at the right line
struct OpenInEditorParams {
    // Path to the input file to open
    input_path: InputPath,
    // Byte offset within the file to position cursor at
    byte_offset: Option<usize>,
    // Line number to position cursor at
    line_number: Option<usize>,
}

merde::derive! {
    impl (Deserialize) for struct OpenInEditorParams { input_path, byte_offset, line_number }
}

/// Opens a file in the configured text editor at the specified line number based on byte offset or line number
pub(crate) async fn serve_open_in_editor(rcx: CubReqImpl, body: axum::body::Bytes) -> LegacyReply {
    if !is_development() {
        return LegacyHttpError::with_status(
            StatusCode::BAD_REQUEST,
            "Open in editor is only available in development",
        )
        .into_legacy_reply();
    }

    let params: OpenInEditorParams = merde::json::from_str(
        std::str::from_utf8(&body[..]).wrap_err("deserializing body of /open-in-editor")?,
    )?;

    let mappings = PathMappings::from_ti(rcx.tenant_ref().ti());
    let disk_path = mappings.to_disk_path(&params.input_path)?;

    // Determine line number from byte offset, line number, or use the whole file
    let line_arg = if let Some(line) = params.line_number {
        format!("{}:{}", disk_path, line)
    } else if let Some(offset) = params.byte_offset {
        let contents = tokio::fs::read_to_string(&disk_path)
            .await
            .wrap_err("reading file to determine line number")?;

        // Count newlines up to the byte offset to determine line number
        let line = contents[..offset].chars().filter(|&c| c == '\n').count() + 1;
        format!("{}:{}", disk_path, line)
    } else {
        disk_path.to_string()
    };

    let editor = "zed";
    tracing::info!("Opening editor {editor} on {line_arg}");

    tokio::spawn(async move {
        let status = tokio::process::Command::new(editor)
            .arg(&line_arg)
            .status()
            .await;

        if let Err(e) = status {
            warn!("Failed to open editor: {}", e);
        }
    });

    "OK".into_legacy_reply()
}
