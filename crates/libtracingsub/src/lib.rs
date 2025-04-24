use autotrait::autotrait;

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn install(&self) {
        use std::str::FromStr;
        use tracing_subscriber::{
            Layer, Registry, filter::Targets, layer::SubscriberExt, util::SubscriberInitExt,
        };

        let rust_log_var =
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,sqlx=warn".to_string());
        let log_filter = Targets::from_str(&rust_log_var).unwrap();

        let with_time = matches!(
            std::env::var("RUST_LOG_TIME").as_deref(),
            Ok("1") | Ok("true") | Ok("yes")
        );

        let layer = tracing_subscriber::fmt::layer()
            .with_ansi(true)
            .with_target(true);

        if with_time {
            Registry::default()
                .with(layer.with_filter(log_filter))
                .init()
        } else {
            let layer = layer.without_time();
            Registry::default()
                .with(layer.with_filter(log_filter))
                .init()
        }

        // Also install env_logger for compatibility with crates using log.
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or(&rust_log_var),
        )
        .is_test(false)
        .try_init();
    }
}
