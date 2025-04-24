use tokio::net::TcpListener;

use config::CubConfig;
use futures_core::future::BoxFuture;

#[derive(Default)]
struct ModImpl;

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
