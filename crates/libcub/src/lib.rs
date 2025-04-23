use tokio::net::TcpListener;

use config::CubConfig;
use futures_core::future::BoxFuture;

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

pub enum OpenBehavior {
    OpenOnStart,
    DontOpen,
}

#[dylo::export]
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
                .map_err(|e| noteyre::eyre!("{}", e))
        })
    }
}

#[cfg(feature = "impl")]
mod impls;

pub type Result<T, E = noteyre::BS> = std::result::Result<T, E>;

include!(".dylo/spec.rs");
include!(".dylo/support.rs");
