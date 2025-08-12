use config_types::Environment;
use sentry::ClientInitGuard;

pub fn install() -> ClientInitGuard {
    let _guard = sentry::init((
        "https://a02afe0f91aa0f0719974fc71834a401@o1172311.ingest.us.sentry.io/4509831845707776",
        sentry::ClientOptions {
            release: sentry::release_name!(),
            // Capture user IPs and potentially sensitive headers when using HTTP server integrations
            // see https://docs.sentry.io/platforms/rust/data-management/data-collected for more info
            send_default_pii: true,
            environment: Some(
                if Environment::default().is_prod() {
                    "production"
                } else {
                    "development"
                }
                .into(),
            ),
            enable_logs: true,
            attach_stacktrace: true,
            default_integrations: true,
            server_name: Some(
                hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "unknown".to_string())
                    .into(),
            ),
            ..Default::default()
        },
    ));
    sentry::capture_message("Hello World!", sentry::Level::Info);
    _guard
}
