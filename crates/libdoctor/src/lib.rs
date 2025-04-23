use futures_core::future::BoxFuture;

#[cfg(feature = "impl")]
mod impls;

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl {}

#[dylo::export]
impl Mod for ModImpl {
    /// Load all modules once to make sure that they actually work
    fn run(&self) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            if let Err(code) = impls::doctor().await {
                eprintln!("doctor failed, exiting with code {code}");
                std::process::exit(code);
            }
        })
    }
}

include!(".dylo/spec.rs");
include!(".dylo/support.rs");
