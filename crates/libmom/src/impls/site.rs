use axum::{
    body::Body,
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
use libterm::FormatAnsiStyle;
use log::error;
use std::{backtrace::Backtrace, borrow::Cow, sync::Arc};

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
    Internal {
        err: String,
    },
}

impl HttpError {
    fn from_report(err: Report) -> Self {
        Self::from_report_ref(&err)
    }

    fn from_report_ref(err: &Report) -> Self {
        let error_unique_id = "err_mom";
        error!(
            "HTTP handler error (chain len {}) {error_unique_id}: {}",
            err.chain().len(),
            err
        );
        for (i, e) in err.chain().enumerate() {
            if i > 0 {
                error!("Caused by: {e}");
            }
        }

        let maybe_bt = liberrhandling::load().format_backtrace_to_terminal_colors(err);
        match maybe_bt.as_ref() {
            Some(bt) => {
                log::error!("Backtrace:\n{bt}");
            }
            None => {
                log::error!("No backtrace :(");
            }
        }

        let body = "Internal server error".to_string();
        HttpError::Internal { err: body }
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

impl From<Arc<eyre::Report>> for HttpError {
    fn from(err: Arc<eyre::Report>) -> Self {
        Self::from_report_ref(&err)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        match self {
            HttpError::WithStatus { status_code, msg } => (status_code, msg).into_response(),
            HttpError::Internal { err } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, ContentType::HTML.as_str())],
                err,
            )
                .into_response(),
        }
    }
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
}
