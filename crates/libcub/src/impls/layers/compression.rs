use axum::{
    body::Body,
    http::{HeaderValue, Request},
    response::IntoResponse,
};
use eyre::Result;
use http::{HeaderMap, Response, StatusCode, header};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};
use tower::{Layer, Service};

#[derive(Clone)]
pub(crate) struct CompressionLayer {
    compress: &'static dyn libcompress::Mod,
}

impl Default for CompressionLayer {
    fn default() -> Self {
        Self {
            compress: libcompress::load(),
        }
    }
}

impl<S> Layer<S> for CompressionLayer {
    type Service = CompressionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CompressionService {
            inner,
            compress: self.compress,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CompressionService<S> {
    inner: S,
    compress: &'static dyn libcompress::Mod,
}

impl<S> Service<Request<Body>> for CompressionService<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let accept_encoding = req.headers().get(header::ACCEPT_ENCODING).cloned();
        let future = self.inner.call(req);
        let compress = self.compress;

        Box::pin(async move {
            Ok(CompressContext {
                res: future.await?,
                accept_encoding,
                compress,
            }
            .call()
            .await)
        })
    }
}

struct CompressContext {
    res: Response<Body>,
    accept_encoding: Option<HeaderValue>,
    compress: &'static dyn libcompress::Mod,
}

impl CompressContext {
    async fn call(self) -> Response<Body> {
        if !should_compress(self.res.headers()) {
            tracing::trace!(
                "Not compressing response with content-type: {:?}",
                self.res.headers().get(http::header::CONTENT_TYPE)
            );
            return self.res;
        }

        let (mut parts, body) = self.res.into_parts();
        let accept_encoding = match self
            .accept_encoding
            .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
        {
            Some(ae) => ae,
            None => {
                tracing::debug!("No Accept-Encoding header");
                return Response::from_parts(parts, body);
            }
        };

        let limit = 128 * 1024 * 1024; // 128 MiB
        let bytes = match axum::body::to_bytes(body, limit).await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!("Failed to buffer response to compress it: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to buffer response to compress it",
                )
                    .into_response();
            }
        };

        let start = Instant::now();
        let old_size = bytes.len();
        if old_size < 100 {
            // Do not compress anything smaller than 100 bytes
            return Response::from_parts(parts, Body::from(bytes));
        }

        let compress_res = self.compress.compress(bytes, &accept_encoding);
        match compress_res {
            Ok(result) => {
                let content_length = result.payload.len();
                let new_size = content_length;

                parts.headers.insert(
                    header::CONTENT_LENGTH,
                    HeaderValue::from_str(&content_length.to_string()).unwrap(),
                );
                if let Some(encoding) = result.content_encoding {
                    tracing::trace!(
                        "🗜️ Spent \x1b[33m{elapsed:?}\x1b[0m on \x1b[36m{encoding}\x1b[0m to save \x1b[32m{savings_percentage:.2}%\x1b[0m (\x1b[35m{old_size}\x1b[0m => \x1b[35m{new_size}\x1b[0m)",
                        elapsed = start.elapsed(),
                        savings_percentage = (1.0 - (new_size as f64 / old_size as f64)) * 100.0,
                    );
                    parts.headers.insert(
                        http::header::CONTENT_ENCODING,
                        HeaderValue::from_static(encoding),
                    );
                }
                Response::from_parts(parts, Body::from(result.payload))
            }
            Err(err) => {
                // If compression fails, log the error and return a 500
                tracing::error!(%err, "Failed to compress response");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to compress response",
                )
                    .into_response()
            }
        }
    }
}

fn should_compress(headers: &HeaderMap) -> bool {
    // Check if the response is already compressed
    if headers.get(http::header::CONTENT_ENCODING).is_some() {
        return false;
    }

    // Do not compress anything smaller than 100 bytes
    if let Some(content_length) = headers.get(http::header::CONTENT_LENGTH) {
        if let Some(length) = content_length
            .to_str()
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
        {
            if length < 100 {
                return false;
            }
        }
    }

    if let Some(content_type) = headers.get(http::header::CONTENT_TYPE) {
        if let Ok(content_type) = content_type.to_str() {
            let essence = content_type.split(';').next().unwrap();

            return matches!(
                essence,
                "text/html"
                    | "text/css"
                    | "text/javascript"
                    | "application/json"
                    | "application/javascript"
                    | "application/xml"
                    | "application/x-font-ttf"
                    | "application/x-font-opentype"
                    | "application/vnd.ms-fontobject"
                    | "image/svg+xml"
                    | "image/x-icon"
                    | "text/plain"
                    | "text/xml"
                    | "application/xhtml+xml"
                    | "application/rss+xml"
                    | "application/atom+xml"
            );
        }
    }
    false
}
