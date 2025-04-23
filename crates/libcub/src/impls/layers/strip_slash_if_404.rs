use axum::{body::Body, extract::Request, http::StatusCode, http::Uri, response::Response};
use futures_core::future::BoxFuture;
use http::header::LOCATION;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Clone)]
pub(crate) struct StripSlashIf404Layer;

impl<S> Layer<S> for StripSlashIf404Layer {
    type Service = StripSlashIf404Service<S>;

    fn layer(&self, service: S) -> Self::Service {
        StripSlashIf404Service { inner: service }
    }
}

#[derive(Clone)]
pub(crate) struct StripSlashIf404Service<S> {
    inner: S,
}

impl<S> Service<Request> for StripSlashIf404Service<S>
where
    S: Service<Request, Response = Response<Body>> + Send + 'static,
    S::Future: Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let original_uri = req.uri().clone();
        let future = self.inner.call(req);

        Box::pin(async move {
            let mut response = future.await?;

            if response.status() == StatusCode::NOT_FOUND
                && original_uri.path().len() > 1
                && original_uri.path().ends_with('/')
            {
                let new_path = original_uri.path().trim_end_matches('/');
                let mut new_uri_parts = original_uri.clone().into_parts();
                new_uri_parts.path_and_query = Some(new_path.parse().unwrap());
                let new_uri = Uri::from_parts(new_uri_parts).unwrap();

                *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
                response
                    .headers_mut()
                    .insert(LOCATION, new_uri.to_string().parse().unwrap());
            }

            Ok(response)
        })
    }
}
