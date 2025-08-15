use std::borrow::Cow;

use axum::{
    body::Body,
    extract::{FromRequest, Request},
    http::{
        StatusCode,
        header::{self, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use content_type::ContentType;
use eyre::Report;
use facet::Facet;
use facet_json::DeserError;
use libhttpclient::header::HeaderName;
use log::error;
use mom_types::MomStructuredError;

pub(crate) type Reply = Result<Response, HttpError>;

pub trait IntoReply {
    fn into_reply(self) -> Reply;
}

impl<T: IntoResponse> IntoReply for T {
    fn into_reply(self) -> Reply {
        Ok(self.into_response())
    }
}

pub struct FacetJson<T>(pub T);

impl<T, S> FromRequest<S> for FacetJson<T>
where
    for<'facet> T: Facet<'facet>,
    S: Send + Sync,
{
    type Rejection = Reply;

    fn from_request(
        req: Request,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        Box::pin(async move {
            let body = match axum::body::Bytes::from_request(req, state).await {
                Ok(body) => body,
                Err(_) => {
                    return Err(HttpError::with_status(
                        StatusCode::BAD_REQUEST,
                        "Failed to read request body",
                    )
                    .into_reply());
                }
            };

            let body = match String::from_utf8(body.to_vec()) {
                Ok(s) => s,
                Err(_) => {
                    return Err(
                        HttpError::with_status(StatusCode::BAD_REQUEST, "Invalid UTF-8")
                            .into_reply(),
                    );
                }
            };

            let body: T = match facet_json::from_str(&body) {
                Ok(obj) => obj,
                Err(err) => {
                    log::error!("JSON deserialization error: {err:?}");
                    log::error!("JSON sent: {body}");
                    return Err(
                        HttpError::with_status(StatusCode::BAD_REQUEST, "Invalid JSON")
                            .into_reply(),
                    );
                }
            };
            // TODO: if error, log error _and_ t?
            Ok(FacetJson(body))
        })
    }
}

impl<'facet, T> IntoReply for FacetJson<T>
where
    T: Facet<'facet>,
{
    fn into_reply(self) -> Reply {
        let payload = facet_json::to_string(&self.0);

        (
            StatusCode::OK,
            [(CONTENT_TYPE, ContentType::JSON.as_str())],
            Body::from(payload),
        )
            .into_reply()
    }
}

#[derive(Debug)]
pub enum HttpError {
    WithStatus {
        status_code: StatusCode,
        msg: Cow<'static, str>,
    },
    Structured {
        payload: MomStructuredError,
    },
}

impl HttpError {
    pub fn with_status<S>(status_code: StatusCode, msg: S) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        HttpError::WithStatus {
            status_code,
            msg: msg.into(),
        }
    }

    fn from_report(err: Report) -> Self {
        let uuid = sentrywrap::capture_report(&err);

        error!(
            "HTTP handler errored: (chain len {}) {uuid}: {}",
            err.chain().len(),
            err
        );
        for (i, e) in err.chain().enumerate() {
            if i > 0 {
                error!("Caused by: {e}");
            }
        }

        let maybe_bt = liberrhandling::load().format_backtrace_to_terminal_colors(&err);
        match maybe_bt.as_ref() {
            Some(bt) => {
                log::error!("Backtrace:\n{bt}");
            }
            None => {
                log::error!("No backtrace :(");
            }
        }

        let mut errors = Vec::new();
        for e in err.chain() {
            errors.push(e.to_string());
        }

        let frames = if let Some(bt) = maybe_bt {
            bt.lines().map(|line| line.to_string()).collect()
        } else {
            vec!["No backtrace available".to_string()]
        };

        let payload = MomStructuredError {
            unique_id: uuid.to_string(),
            errors,
            frames,
        };
        HttpError::Structured { payload }
    }
}

macro_rules! impl_from {
    ($from:ty) => {
        impl From<$from> for HttpError {
            fn from(err: $from) -> Self {
                Self::from_report(err.into())
            }
        }
    };
}

impl_from!(std::io::Error);
impl_from!(eyre::Report);
impl_from!(axum::http::Error);
impl_from!(axum::http::header::InvalidHeaderValue);
impl_from!(axum::http::uri::InvalidUri);
impl_from!(url::ParseError);
impl_from!(r2d2::Error);
impl_from!(rusqlite::Error);
impl_from!(libobjectstore::Error);
impl_from!(std::str::Utf8Error);

impl<'input> From<DeserError<'input>> for HttpError {
    fn from(err: DeserError<'input>) -> Self {
        Self::from_report(eyre::eyre!("{err}"))
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        match self {
            HttpError::WithStatus { status_code, msg } => (status_code, msg).into_response(),
            HttpError::Structured { payload } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [
                    (header::CONTENT_TYPE, ContentType::JSON.as_str()),
                    (HeaderName::from_static("x-mom-structured-error"), "1"),
                ],
                Body::from(facet_json::to_string(&payload)),
            )
                .into_response(),
        }
    }
}
