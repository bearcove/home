use axum::{
    body::Body,
    http::{HeaderName, StatusCode, header},
    response::{IntoResponse, Response},
};
use config_types::is_production;
use conflux::RevisionError;
use content_type::ContentType;
use eyre::Report;
use http::header::CONTENT_TYPE;
use libterm::FormatAnsiStyle;
use merde::{DynSerialize, IntoStatic};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::borrow::Cow;
use tracing::error;
use ulid::Ulid;

/// The type returned by HTTP handlers in our application
///
/// This is a `Result` where:
/// - The success case is an HTTP response
/// - The error case is an `HttpError` that can be automatically converted into an HTTP response with appropriate status code
pub(crate) type LegacyReply = Result<Response, LegacyHttpError>;

/// Trait for converting a value into a `Reply`
///
/// This is similar to axum's `IntoResponse`, but with a key difference:
/// - `IntoResponse` returns a `Response` directly
/// - `IntoReply` returns a `Result<Response, HttpError>` which allows for error handling
///
/// The benefit of `IntoReply` is that `HttpError` implements `From<E>` for many error types,
/// which makes error handling and propagation much more convenient using the `?` operator.
pub trait IntoLegacyReply {
    fn into_legacy_reply(self) -> LegacyReply;
}

/// Implement `IntoReply` for anything that implements axum's `IntoResponse`
///
/// This allows using axum's response types directly with our `into_reply()` API
impl<T: IntoResponse> IntoLegacyReply for T {
    fn into_legacy_reply(self) -> LegacyReply {
        Ok(self.into_response())
    }
}

pub struct MerdeJson<T>(pub T);

impl<T> IntoLegacyReply for MerdeJson<T>
where
    T: DynSerialize,
{
    fn into_legacy_reply(self) -> LegacyReply {
        let payload = merde::json::to_vec(&self.0)?;

        (
            StatusCode::OK,
            [(CONTENT_TYPE, ContentType::JSON.as_str())],
            Body::from(payload),
        )
            .into_legacy_reply()
    }
}

#[derive(Debug)]
pub enum LegacyHttpError {
    WithStatus {
        status_code: StatusCode,
        msg: Cow<'static, str>,
    },
    Internal {
        err: String,
    },
}

impl LegacyHttpError {
    fn from_report(err: Report) -> Self {
        let error_unique_id = format!("snafu_{}", Ulid::new().to_string().to_lowercase());
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

        let maybe_bt = liberrhandling::load().format_backtrace_to_terminal_colors(&err);

        let mut trace_content = {
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

            let term = libterm::load();
            let mut err_string = term.format_ansi(&err_string, FormatAnsiStyle::Html);

            // Replace markdown-style links with HTML anchor tags
            // Syntax: [text](home://path/to/file) -> <a href="home://path/to/file">text</a>
            err_string = regex::Regex::new(r"\[([^\]]+)\]\(home://([^)]+)\)")
                .expect("Failed to compile regex")
                .replace_all(&err_string, |caps: &regex::Captures| {
                    let text = &caps[1];
                    let path = &caps[2];
                    format!(r#"<a href="home://{path}">{text}</a>"#)
                })
                .to_string();

            let backtrace = maybe_bt.unwrap_or_default();
            let backtrace = term.format_ansi(&backtrace, FormatAnsiStyle::Html);

            format!(
                r#"<pre class="trace home-ansi">{err_string}<div class="backtrace">{backtrace}</div></pre>"#
            )
        };
        tracing::error!("Backtrace:\n{trace_content}");
        if is_production() {
            trace_content = "".into();
        }

        let date = time::OffsetDateTime::now_utc()
            .format(&time::format_description::parse(
                "[month repr:short] [day], [year] at [hour repr:12]:[minute][period case:upper] UTC"
            ).unwrap())
            .unwrap();

        let sadmojis = [
            "😩", "😭", "😢", "😖", "😣", "😞", "😓", "😔", "☹️", "😧", "🥺", "🤕",
        ];
        let sadmoji = *sadmojis.choose(&mut thread_rng()).unwrap();
        let color_css = libterm::load().css();

        let body = format!(
            r#"
            <html>
            {padding}
            <head>
                <title>Internal server error</title>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <meta http-equiv="X-UA-Compatible" content="ie=edge">
                <meta name="description" content="Internal server error page">
                <meta name="robots" content="noindex, nofollow">
                <script src="/internal-api/builtins/livereload.js"></script>
                <link rel="stylesheet" href="/internal-api/builtins/ansi.css">
                <style id="sass-bundle">
                    @media (prefers-color-scheme: dark) {{
                        :root {{
                            color-scheme: dark;
                        }}
                    }}

                    @media (prefers-color-scheme: light) {{
                        :root {{
                            color-scheme: light;
                        }}
                    }}

                    body {{
                        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
                            Helvetica, Arial, sans-serif, "Apple Color Emoji", "Segoe UI Emoji",
                            "Segoe UI Symbol";
                        font-size: 14pt;
                        margin: 0;
                        line-height: 1.4;

                        background: light-dark(#ffffff, #000000);
                        color: light-dark(#2c3e50, #ecf0f1);
                    }}

                    div.content {{
                        margin: 0 auto;
                        max-width: 100%;
                        min-height: 100vh;
                        padding: 1em 2em;

                        background: light-dark(#f7f7f7, #181818);
                    }}

                    h1 {{
                        margin: 0;
                    }}

                    a, a:visited {{
                    color: light-dark(#3498db, #5dade2);
                    }}

                    pre.trace {{
                        font-family: monospace;
                        color: light-dark(#333333, #d9d9d9);
                        white-space: pre-wrap;
                        overflow-x: auto;
                        font-size: .85rem;
                        padding: 0;
                    }}

                    section.info {{
                        font-size: .8rem;
                        display: flex;
                        gap: 1em;

                        p {{
                            margin: 0.3em 0;
                        }}
                    }}

                    @keyframes sadmoji-pulse {{
                        0% {{ transform: scale(1); }}
                        50% {{ transform: scale(1.2); }}
                        100% {{ transform: scale(1); }}
                    }}

                    .sadmoji {{
                        display: inline-block;
                        animation: sadmoji-pulse 0.4s ease-in-out;
                    }}

                    {color_css}
                </style>
            </head>
            <body>
                <div class="content">
                    <h1>Internal server error <span class="sadmoji">{sadmoji}</span></h1>

                    <section class="info">
                        <p>📆 <strong>{date}</strong></p>
                        <p>🆔 <strong><code>{error_unique_id}</code></strong></p>
                    </section>

                    {trace_content}

                    <p>
                        Hopefully <a href="/">the homepage</a> still works.
                    </p>
                </div>
            </body>
            </html>
            "#,
            padding = "<!-- Padding to avoid browser 500 error -->\n".repeat(10),
            trace_content = trace_content.trim()
        );

        LegacyHttpError::Internal { err: body }
    }
}

macro_rules! impl_from {
    ($from:ty) => {
        impl From<$from> for LegacyHttpError {
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
impl_from!(libobjectstore::Error);
impl_from!(std::str::Utf8Error);
impl_from!(std::string::FromUtf8Error);

impl<'s> From<merde::MerdeError<'s>> for LegacyHttpError {
    fn from(err: merde::MerdeError<'s>) -> Self {
        Self::from_report(err.into_static().into())
    }
}

impl From<RevisionError> for LegacyHttpError {
    fn from(err: RevisionError) -> Self {
        Self::from_report(err.into())
    }
}

impl IntoResponse for LegacyHttpError {
    fn into_response(self) -> Response {
        match self {
            LegacyHttpError::WithStatus { status_code, msg } => (status_code, msg).into_response(),
            LegacyHttpError::Internal { err } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, ContentType::HTML.as_str())],
                err,
            )
                .into_response(),
        }
    }
}

impl LegacyHttpError {
    pub fn with_status<S>(status_code: StatusCode, msg: S) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        LegacyHttpError::WithStatus {
            status_code,
            msg: msg.into(),
        }
    }
}

/// The two genders^W cache-control header: cache forever or don't cache at all.
pub enum ClientCachePolicy {
    // the URL is cache-busted (it includes the hash bit of the hapa), so we can send a long max-age
    CacheBasicallyForever,
}

impl ClientCachePolicy {
    pub fn to_max_age(&self) -> &'static str {
        match self {
            ClientCachePolicy::CacheBasicallyForever => "max-age=31536000",
        }
    }

    pub fn to_header_tuple(&self) -> (HeaderName, &'static str) {
        (header::CACHE_CONTROL, self.to_max_age())
    }
}
