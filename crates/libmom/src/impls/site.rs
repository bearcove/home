use axum::{
    body::Body,
    http::{
        header::{self, CONTENT_TYPE},
        StatusCode,
    },
    response::{IntoResponse, Response},
};
use content_type::ContentType;
use eyre::Report;
use merde::{DynSerialize, IntoStatic as _};
use std::{borrow::Cow, sync::Arc};
use term::FormatAnsiStyle;
use tracing::error;

pub(crate) type Reply = Result<Response, HttpError>;

pub trait IntoReply {
    fn into_reply(self) -> Reply;
}

impl<T: IntoResponse> IntoReply for T {
    fn into_reply(self) -> Reply {
        Ok(self.into_response())
    }
}

pub struct MerdeJson<T>(pub T);

impl<T> IntoReply for MerdeJson<T>
where
    T: DynSerialize,
{
    fn into_reply(self) -> Reply {
        let payload = merde::json::to_vec(&self.0)?;
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
                error!("Caused by: {}", e);
            }
        }

        let maybe_bt = errhandling::load().format_backtrace_to_terminal_colors(err);

        let trace_content = {
            let mut err_string = String::new();
            let num_errors_in_chain = err.chain().count();
            if num_errors_in_chain == 1 {
                err_string = err.to_string();
            } else {
                for (i, e) in err.chain().enumerate() {
                    use std::fmt::Write;
                    let error = &e.to_string();
                    let err_lines = error.lines().collect::<Vec<_>>();
                    let _ = writeln!(&mut err_string, "\x1b[32m{}.\x1b[0m {}", i + 1, e);
                    for (j, line) in err_lines.iter().enumerate() {
                        if j > 0 {
                            let _ = writeln!(&mut err_string, "   {}", line);
                        }
                    }
                }
            }

            let err_string_colored = term::load().format_ansi(&err_string, FormatAnsiStyle::Html);
            let backtrace = maybe_bt.unwrap_or_default();

            format!(
                r#"<pre class="trace home-ansi">{err_string_colored}<div class="backtrace">{backtrace}</div></pre>"#
            )
        };
        tracing::error!("Backtrace:\n{trace_content}");

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
impl_from!(objectstore::Error);
impl_from!(std::str::Utf8Error);

impl From<merde::MerdeError<'_>> for HttpError {
    fn from(err: merde::MerdeError<'_>) -> Self {
        Self::from_report(err.into_static().into())
    }
}

impl From<eyre::BS> for HttpError {
    fn from(err: eyre::BS) -> Self {
        Self::from_report(err.into())
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
