use axum::response::IntoResponse as _;
use config::is_development;
use http::{StatusCode, Uri};
use std::collections::HashMap;

use crate::impls::reply::{IntoLegacyReply, LegacyHttpError, LegacyReply};

pub(crate) async fn download_url(
    query: axum::extract::Query<HashMap<String, String>>,
) -> LegacyReply {
    if !is_development() {
        return LegacyHttpError::with_status(
            StatusCode::BAD_REQUEST,
            "Download URL is only available in development",
        )
        .into_legacy_reply();
    }

    let url = match query.get("url") {
        Some(value) => value,
        None => {
            return LegacyHttpError::with_status(StatusCode::BAD_REQUEST, "Missing url parameter")
                .into_legacy_reply();
        }
    };

    let uri = url.parse::<Uri>().unwrap();
    let client = libhttpclient::load().client();
    let response = match client
        .get(uri)
        .browser_like_user_agent()
        .send_and_expect_200()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return LegacyHttpError::with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to download URL: {}", e),
            )
            .into_legacy_reply();
        }
    };

    let headers = response.headers_only_string_safe();
    let content_type = headers
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let body = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return LegacyHttpError::with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read response body: {}", e),
            )
            .into_legacy_reply();
        }
    };

    let resp = (
        [
            (http::header::CONTENT_TYPE, content_type.as_str()),
            (http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (
                http::header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, OPTIONS",
            ),
            (http::header::ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type"),
        ],
        body,
    );

    Ok(resp.into_response())
}
