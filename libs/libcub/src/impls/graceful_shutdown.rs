use config::Environment;
use tokio::signal::unix::SignalKind;
use tracing::{error, warn};

/// This async function returns or resolves whenever we receive sigint or sigterm.
pub(crate) async fn setup_graceful_shutdown() {
    let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = sigint.recv() => {
            warn!("Received SIGINT");
        },
        _ = sigterm.recv() => {
            warn!("Received SIGTERM");
        },
    }
    if Environment::default().is_dev() {
        warn!("Exiting immediately");
        std::process::exit(0);
    }

    warn!("Initiating graceful shutdown");

    tokio::spawn(async move {
        sigint.recv().await;
        error!("Received second signal, exiting ungracefully");
        std::process::exit(1);
    });
}
