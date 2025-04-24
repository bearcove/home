use autotrait::autotrait;
use futures_core::future::BoxFuture;

#[derive(Default)]
struct ModImpl;

pub use eyre::Result;
use mom_types::MomServeArgs;

#[autotrait]
impl Mod for ModImpl {
    fn serve(&self, args: MomServeArgs) -> BoxFuture<Result<()>> {
        Box::pin(impls::serve(args))
    }
}

pub(crate) mod impls;
