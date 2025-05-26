use crate::impls::reply::{IntoLegacyReply, LegacyHttpError, LegacyReply};
use config_types::is_development;
use eyre::Context as _;
use facet::Facet;
use http::StatusCode;
use log::{info, warn};

/// Params for writing text to system clipboard
#[derive(Facet)]
struct WriteToClipboardParams {
    text: String,
}

/// Writes provided text to the system clipboard
pub(crate) async fn serve_write_to_clipboard(body: axum::body::Bytes) -> LegacyReply {
    if !is_development() {
        return LegacyHttpError::with_status(
            StatusCode::BAD_REQUEST,
            "Write to clipboard is only available in development",
        )
        .into_legacy_reply();
    }

    let params: WriteToClipboardParams = facet_json::from_str(
        std::str::from_utf8(&body[..]).wrap_err("deserializing body of /write-to-clipboard")?,
    )?;

    tokio::task::spawn_blocking(move || {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => match clipboard.set_text(&params.text) {
                Ok(_) => {
                    info!(
                        "Successfully wrote {} bytes to clipboard",
                        params.text.len()
                    );
                }
                Err(e) => {
                    warn!("Failed to write to clipboard: {e}");
                    return Err(eyre::eyre!("Failed to write to clipboard: {}", e));
                }
            },
            Err(e) => {
                warn!("Failed to access clipboard: {e}");
                return Err(eyre::eyre!("Failed to access clipboard: {}", e));
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| eyre::eyre!("Failed to join blocking task: {}", e))??;

    "OK".into_legacy_reply()
}
