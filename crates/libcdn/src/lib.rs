use cub_types::CubReq;
use futures_core::future::BoxFuture;
use hattip::{http::HeaderMap, HReply};

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

#[cfg(feature = "impl")]
mod impls;

#[dylo::export]
impl Mod for ModImpl {
    fn serve_asset(&self, rcx: Box<dyn CubReq>, headers: HeaderMap) -> BoxFuture<'_, HReply> {
        Box::pin(async move { impls::serve_asset(rcx, headers).await })
    }
}

include!(".dylo/spec.rs");
include!(".dylo/support.rs");
