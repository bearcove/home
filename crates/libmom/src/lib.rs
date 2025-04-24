use autotrait::autotrait;
use futures_core::future::BoxFuture;

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    &ModImpl
}

pub use eyre::Result;
use mom_types::MomServeArgs;

#[autotrait]
impl Mod for ModImpl {
    fn serve(&self, args: MomServeArgs) -> BoxFuture<Result<()>> {
        Box::pin(impls::serve(args))
    }
}

pub(crate) mod impls;
