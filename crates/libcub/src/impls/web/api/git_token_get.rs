use crate::impls::reply::{LegacyHttpError, LegacyReply};
use http::StatusCode;

pub async fn serve_git_token_info() -> LegacyReply {
    Err(LegacyHttpError::with_status(
        StatusCode::METHOD_NOT_ALLOWED,
        "Use POST method with {\"repo\": \"repo-name\"} to generate a git clone token",
    ))
}