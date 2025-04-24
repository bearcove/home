pub mod prelude;

pub use bytes;
pub use http;
use http::{Response, StatusCode};
use std::borrow::Cow;

#[derive(Debug)]
pub enum HError {
    WithStatus {
        status_code: StatusCode,
        msg: Cow<'static, str>,
    },
    Internal {
        err: String,
    },
}

impl HError {
    pub fn with_status(status_code: StatusCode, msg: impl Into<Cow<'static, str>>) -> Self {
        Self::WithStatus {
            status_code,
            msg: msg.into(),
        }
    }

    pub fn bad_request(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::WithStatus {
            status_code: StatusCode::BAD_REQUEST,
            msg: msg.into(),
        }
    }

    pub fn internal(err: impl ToString) -> Self {
        Self::Internal {
            err: err.to_string(),
        }
    }
}

pub type HReply = Result<HResponse, HError>;
pub type HResponse = Response<HBody>;

#[derive(Default)]
pub enum HBody {
    #[default]
    Empty,
    String(String),
    VecU8(Vec<u8>),
    Bytes(bytes::Bytes),
}

impl HBody {
    pub fn empty() -> Self {
        HBody::Empty
    }
}

impl From<&'static str> for HBody {
    fn from(s: &'static str) -> Self {
        HBody::String(s.to_string())
    }
}

impl From<String> for HBody {
    fn from(s: String) -> Self {
        HBody::String(s)
    }
}

impl From<Vec<u8>> for HBody {
    fn from(v: Vec<u8>) -> Self {
        HBody::VecU8(v)
    }
}

impl From<bytes::Bytes> for HBody {
    fn from(b: bytes::Bytes) -> Self {
        HBody::Bytes(b)
    }
}
impl From<bytes::BytesMut> for HBody {
    fn from(bytes: bytes::BytesMut) -> Self {
        HBody::Bytes(bytes.freeze())
    }
}

pub trait IntoLightReply {
    fn into_reply(self) -> HReply;
}

impl IntoLightReply for Result<HResponse, http::Error> {
    fn into_reply(self) -> HReply {
        match self {
            Ok(response) => Ok(response),
            Err(err) => Err(HError::Internal {
                err: err.to_string(),
            }),
        }
    }
}

impl IntoLightReply for Response<HBody> {
    fn into_reply(self) -> HReply {
        Ok(self)
    }
}

impl IntoLightReply for HError {
    fn into_reply(self) -> HReply {
        Err(self)
    }
}

impl IntoLightReply for StatusCode {
    fn into_reply(self) -> HReply {
        Err(HError::WithStatus {
            status_code: self,
            msg: Cow::Borrowed(""),
        })
    }
}

impl IntoLightReply for String {
    fn into_reply(self) -> HReply {
        Err(HError::Internal { err: self })
    }
}

impl IntoLightReply for &'static str {
    fn into_reply(self) -> HReply {
        Err(HError::Internal {
            err: self.to_string(),
        })
    }
}

pub fn to_herror(d: impl std::fmt::Display) -> HError {
    HError::Internal { err: d.to_string() }
}

pub fn redirect(u: &str) -> HReply {
    Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(http::header::LOCATION, u)
        .body(HBody::String(format!("Redirecting to {u}")))
        .map_err(|e| HError::Internal { err: e.to_string() })
}
