use autotrait::autotrait;
pub use bytes::Bytes;
use futures_core::{future::BoxFuture, stream::BoxStream};
use merde::{DynSerialize, MerdeError};
use std::collections::HashMap;

pub use form_urlencoded;
pub use http::{
    HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header, request, response,
};

#[derive(Debug)]
pub enum Error {
    /// Any other error
    Any(String),

    /// JSON parsing error
    Json(String),

    /// HTTP error
    Non200Status {
        status: StatusCode,
        response: String,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Any(s) => write!(f, "{s}"),
            Error::Json(s) => write!(f, "{s}"),
            Error::Non200Status { status, response } => {
                write!(f, "HTTP Status {status}: {response}")
            }
        }
    }
}

impl From<eyre::Error> for Error {
    fn from(err: eyre::Error) -> Self {
        Error::Any(err.to_string())
    }
}

impl From<merde::MerdeError<'_>> for Error {
    fn from(err: merde::MerdeError<'_>) -> Self {
        Error::Any(err.to_string())
    }
}

impl std::error::Error for Error {}

#[derive(Clone)]
pub struct ClientOpts {
    pub resolve_to_addrs: HashMap<String, Vec<std::net::SocketAddr>>,
    pub follow_redirects: bool,
}

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[derive(Default)]
struct ModImpl;

#[autotrait]
impl Mod for ModImpl {
    fn client(&self) -> Box<dyn HttpClient> {
        Box::new(HttpClientImpl::new(None))
    }

    fn client_with_opts(&self, opts: ClientOpts) -> Box<dyn HttpClient> {
        Box::new(HttpClientImpl::new(Some(opts)))
    }
}

struct HttpClientImpl {
    client: reqwest::Client,
}

impl HttpClientImpl {
    fn new(opts: Option<ClientOpts>) -> Self {
        let mut builder = reqwest::Client::builder();
        if let Some(opts) = opts {
            for (host, addrs) in opts.resolve_to_addrs {
                builder = builder.resolve_to_addrs(&host, &addrs);
            }
            if opts.follow_redirects {
                builder = builder.redirect(reqwest::redirect::Policy::limited(10));
            } else {
                builder = builder.redirect(reqwest::redirect::Policy::none());
            }
        }
        Self {
            client: builder.build().unwrap(),
        }
    }
}

#[autotrait]
impl HttpClient for HttpClientImpl {
    fn request(&self, method: Method, uri: Uri) -> Box<dyn RequestBuilder> {
        Box::new(RequestBuilderImpl {
            client: self.client.clone(),
            method,
            uri,
            headers: Default::default(),
            body: None,
            form: None,
            auth: None,
        })
    }

    fn get(&self, uri: Uri) -> Box<dyn RequestBuilder> {
        self.request(Method::GET, uri)
    }

    fn post(&self, uri: Uri) -> Box<dyn RequestBuilder> {
        self.request(Method::POST, uri)
    }

    fn put(&self, uri: Uri) -> Box<dyn RequestBuilder> {
        self.request(Method::PUT, uri)
    }

    fn delete(&self, uri: Uri) -> Box<dyn RequestBuilder> {
        self.request(Method::DELETE, uri)
    }
}

struct RequestBuilderImpl {
    client: reqwest::Client,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Option<Bytes>,
    form: Option<String>,
    auth: Option<(String, Option<String>)>,
}

#[autotrait]
impl RequestBuilder for RequestBuilderImpl {
    fn body(mut self: Box<Self>, body: Bytes) -> Box<dyn RequestBuilder> {
        self.body = Some(body);
        self
    }

    fn form(mut self: Box<Self>, form: String) -> Box<dyn RequestBuilder> {
        self.form = Some(form);
        self.headers.insert(
            header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        self
    }

    fn header(mut self: Box<Self>, key: HeaderName, value: HeaderValue) -> Box<dyn RequestBuilder> {
        self.headers.insert(key, value);
        self
    }

    /// Sets a "polite" user agent, letting the server know where to reach us.
    fn polite_user_agent(mut self: Box<Self>) -> Box<dyn RequestBuilder> {
        const POLITE_USER_AGENT: HeaderValue =
            HeaderValue::from_static("home/1.0 (home/1.0 +https://home.bearcove.eu)");

        self.headers.insert(header::USER_AGENT, POLITE_USER_AGENT);
        self
    }

    /// Sets a browser-like user agent
    fn browser_like_user_agent(mut self: Box<Self>) -> Box<dyn RequestBuilder> {
        const BROWSER_LIKE_USER_AGENT: HeaderValue = HeaderValue::from_static(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.2 Safari/605.1.15",
        );

        self.headers
            .insert(header::USER_AGENT, BROWSER_LIKE_USER_AGENT);
        self
    }

    fn basic_auth(
        mut self: Box<Self>,
        username: &str,
        password: Option<&str>,
    ) -> Box<dyn RequestBuilder> {
        self.auth = Some((username.to_string(), password.map(String::from)));
        self
    }

    fn bearer_auth(mut self: Box<Self>, token: &str) -> Box<dyn RequestBuilder> {
        self.auth = Some((token.to_string(), None));
        self
    }

    fn send(self: Box<Self>) -> BoxFuture<'static, Result<Box<dyn Response>, Error>> {
        let method = self.method.clone();
        let uri = self.uri.clone();
        let headers = self.headers.clone();
        let body = self.body.clone();
        let form = self.form.clone();
        let auth = self.auth.clone();

        Box::pin(async move {
            let mut request = self.client.request(method, uri.to_string());

            request = request.headers(headers);

            if let Some(body) = body {
                request = request.body(body);
            }

            if let Some(form) = form {
                request = request.body(form);
            }

            if let Some((username, password)) = auth {
                match password {
                    Some(password) => {
                        request = request.basic_auth(username, Some(password));
                    }
                    None => {
                        request = request.bearer_auth(&username);
                    }
                }
            }

            let response = request
                .send()
                .await
                .map_err(|e| Error::Any(e.to_string()))?;
            Ok(Box::new(ResponseImpl::new(response)) as Box<dyn Response>)
        })
    }

    fn send_and_expect_200(
        self: Box<Self>,
    ) -> BoxFuture<'static, Result<Box<dyn Response>, Error>> {
        Box::pin(async move {
            let response = self.send().await.map_err(|e| Error::Any(e.to_string()))?;

            let status = response.status();
            if !status.is_success() {
                let bytes = response.bytes().await?;
                let response_body = match String::from_utf8(bytes.clone()) {
                    Ok(s) => s,
                    Err(_) => {
                        let prefix = bytes
                            .iter()
                            .take(128)
                            .map(|b| format!("{b:02x}"))
                            .collect::<Vec<String>>()
                            .join(" ");
                        format!(
                            "(Response body is not valid UTF-8. First 128 bytes (hex): {prefix})"
                        )
                    }
                };
                Err(Error::Non200Status {
                    status,
                    response: response_body,
                })
            } else {
                Ok(response)
            }
        })
    }

    fn json(
        self: Box<Self>,
        body: &dyn DynSerialize,
    ) -> Result<Box<dyn RequestBuilder>, MerdeError<'static>> {
        let body = merde::json::to_vec(body)?;
        Ok(self
            .header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/json; charset=utf-8"),
            )
            .body(Bytes::from(body)))
    }

    fn query(self: Box<Self>, params: &[(&str, &str)]) -> Box<dyn RequestBuilder> {
        let encoded = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params)
            .finish();
        self.form(encoded)
    }
}

struct ResponseImpl {
    response: reqwest::Response,
}

impl ResponseImpl {
    fn new(response: reqwest::Response) -> Self {
        Self { response }
    }
}

#[autotrait]
impl Response for ResponseImpl {
    fn status(&self) -> StatusCode {
        self.response.status()
    }

    fn headers_only_string_safe(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        for (key, value) in self.response.headers() {
            if let Ok(v) = value.to_str() {
                headers.insert(key.to_string(), v.to_string());
            }
        }
        headers
    }

    fn bytes(self: Box<Self>) -> BoxFuture<'static, Result<Vec<u8>, Error>> {
        let response = self.response;
        Box::pin(async move {
            response
                .bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| Error::Any(e.to_string()))
        })
    }

    fn bytes_stream(self: Box<Self>) -> BoxStream<'static, Result<Bytes, Error>> {
        use futures_util::StreamExt;
        Box::pin(
            self.response
                .bytes_stream()
                .map(|r| r.map_err(|e| Error::Any(e.to_string()))),
        )
    }

    fn text(self: Box<Self>) -> BoxFuture<'static, Result<String, Error>> {
        Box::pin(async move {
            let bytes = self.bytes().await?;
            String::from_utf8(bytes)
                .map_err(|e| Error::Any(format!("Response body is not valid UTF-8: {e}")))
        })
    }
}

impl dyn Response {
    pub fn json<T: merde::DeserializeOwned>(
        self: Box<Self>,
    ) -> BoxFuture<'static, Result<T, Error>> {
        Box::pin(async move {
            let bytes = self.bytes().await?;
            merde::json::from_bytes_owned(&bytes).map_err(|e| Error::Json(e.to_string()))
        })
    }
}
