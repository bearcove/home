use std::task::{Context, Poll};

use axum::{
    body::Body,
    http::{Request, Response},
    response::IntoResponse as _,
};
use tower::{Layer, Service};

use crate::impls::{global_state, host_extract::ExtractedHost, types::DomainResolution};

/// Layer that checks for domain aliases and redirects to the canonical domain if needed
#[derive(Clone)]
pub struct DomainRedirectLayer;

impl<S> Layer<S> for DomainRedirectLayer {
    type Service = DomainRedirectService<S>;

    fn layer(&self, service: S) -> Self::Service {
        DomainRedirectService { inner: service }
    }
}

#[derive(Clone)]
pub struct DomainRedirectService<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for DomainRedirectService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = futures_core::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // Check if we need to redirect based on the domain
        let host = match ExtractedHost::from_headers(req.uri(), req.headers()) {
            Some(host) => host,
            None => {
                // No host header, just pass through
                return Box::pin(self.inner.call(req));
            }
        };

        let domain = host.domain();

        // Check if this domain needs to be redirected
        match host.resolve_domain() {
            Some(DomainResolution::Redirect { target_domain, .. }) => {
                // Build the redirect URL
                let redirect_url =
                    global_state().build_redirect_url(&target_domain, req.uri(), &host.0);

                // Create temporary redirect response (307)
                let response =
                    axum::response::Redirect::temporary(redirect_url.as_str()).into_response();

                tracing::info!("Redirecting {} to {}", domain, redirect_url);
                Box::pin(async move { Ok(response) })
            }
            _ => {
                // No redirect needed, pass through to the inner service
                Box::pin(self.inner.call(req))
            }
        }
    }
}
