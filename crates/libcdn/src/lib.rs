use cub_types::CubReq;
use futures_core::future::BoxFuture;
use hattip::{HReply, http::HeaderMap};

#[derive(Default)]
struct ModImpl;

mod impls;

#[autotrait]
impl Mod for ModImpl {
    fn serve_asset(&self, rcx: Box<dyn CubReq>, headers: HeaderMap) -> BoxFuture<'_, HReply> {
        Box::pin(async move { impls::serve_asset(rcx, headers).await })
    }
}
