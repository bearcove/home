use autotrait::autotrait;
use tokio::net::TcpListener;

use config_types::CubConfig;
use futures_core::future::BoxFuture;

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static INSTANCE: ModImpl = ModImpl;
    &INSTANCE
}

pub enum OpenBehavior {
    OpenOnStart,
    DontOpen,
}

#[autotrait]
impl Mod for ModImpl {
    fn serve(
        &self,
        config: CubConfig,
        ln: TcpListener,
        open_behavior: OpenBehavior,
    ) -> BoxFuture<'static, Result<()>> {
        Box::pin(async {
            impls::serve(config, ln, open_behavior)
                .await
                .map_err(|e| eyre::eyre!("{}", e))
        })
    }
}

mod impls;

pub use eyre::Result;
