use cub_types::CubReq;
use hattip::prelude::*;

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

#[cfg(feature = "impl")]
mod impls;

#[dylo::export]
impl Mod for ModImpl {
    fn serve_comments(&self, rcx: Box<dyn CubReq>) -> BoxFuture<'_, HReply> {
        Box::pin(async move { impls::serve_comments(rcx).await })
    }
}

include!(".dylo/spec.rs");
include!(".dylo/support.rs");
