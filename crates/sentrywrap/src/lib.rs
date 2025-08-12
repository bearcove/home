use config_types::Environment;
use sentry::ClientInitGuard;

#[must_use]
pub fn install() -> ClientInitGuard {
    // copy-pasted across home-mom and home-serve
    sentry::init((
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
            ..Default::default()
        },
    ))
}
