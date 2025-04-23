use axum::response::Response;
use http::{HeaderName, HeaderValue};
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Clone)]
pub(crate) struct SetResponseHeaderLayer {
    name: HeaderName,
    value: HeaderValue,
}

impl SetResponseHeaderLayer {
    pub fn overriding(name: HeaderName, value: HeaderValue) -> Self {
        Self { name, value }
    }
}

impl<S> Layer<S> for SetResponseHeaderLayer {
    type Service = SetResponseHeaderService<S>;

    fn layer(&self, service: S) -> Self::Service {
        SetResponseHeaderService {
            inner: service,
            name: self.name.clone(),
            value: self.value.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct SetResponseHeaderService<S> {
    inner: S,
    name: HeaderName,
    value: HeaderValue,
}

impl<S, ReqBody, ResBody> Service<axum::extract::Request<ReqBody>> for SetResponseHeaderService<S>
where
    S: Service<axum::extract::Request<ReqBody>, Response = axum::response::Response<ResBody>>,
    ReqBody: Send + 'static,
    ResBody: axum::body::HttpBody + Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = SetResponseHeaderFuture<S::Future, ResBody>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: axum::extract::Request<ReqBody>) -> Self::Future {
        SetResponseHeaderFuture {
            inner: self.inner.call(request),
            name: self.name.clone(),
            value: self.value.clone(),
            _phantom: std::marker::PhantomData,
        }
    }
}

pin_project! {
    pub struct SetResponseHeaderFuture<F, B> {
        #[pin]
        inner: F,
        name: HeaderName,
        value: HeaderValue,
        _phantom: std::marker::PhantomData<B>,
    }
}

impl<F, B, E> Future for SetResponseHeaderFuture<F, B>
where
    F: Future<Output = Result<Response<B>, E>>,
    B: axum::body::HttpBody,
{
    type Output = Result<Response<B>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.inner.poll(cx) {
            Poll::Ready(Ok(mut response)) => {
                response
                    .headers_mut()
                    .insert(this.name.clone(), this.value.clone());
                Poll::Ready(Ok(response))
            }
            other => other,
        }
    }
}
