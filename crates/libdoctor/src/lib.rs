use autotrait::autotrait;
use futures_core::future::BoxFuture;

mod impls;

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
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
