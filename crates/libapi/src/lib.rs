use autotrait::autotrait;
use cub_types::CubReq;
use hattip::prelude::*;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    &ModImpl
}

mod impls;

#[autotrait]
impl Mod for ModImpl {
    fn serve_comments(&self, rcx: Box<dyn CubReq>) -> BoxFuture<'_, HReply> {
        Box::pin(async move { impls::serve_comments(rcx).await })
    }
}
