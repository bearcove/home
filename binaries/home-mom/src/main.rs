use std::collections::HashMap;

use camino::Utf8PathBuf;
use config_types::{Environment, TenantConfig, TenantDomain, TenantInfo, WebConfig};
use facet::Facet;
use facet_pretty::FacetPretty;
use mom_types::MomServeArgs;
use skelly::{eyre, log};
use tokio::net::TcpListener;

#[derive(Facet)]
struct Args {
    #[facet(long)]
    /// mom config file
    pub mom_config: Utf8PathBuf,

    #[facet(long)]
    /// tenant config file
    pub tenant_config: Utf8PathBuf,

    #[facet(long, default)]
    /// Unix socket file descriptor for receiving the TCP listener
    pub socket_fd: Option<i32>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    real_main().await
}

async fn real_main() -> eyre::Result<()> {
    skelly::setup();

    let args = std::env::args().skip(1).collect::<Vec<String>>();
    let args_str: Vec<&'static str> = args
        .into_iter()
        .map(|s| Box::leak(s.into_boxed_str()) as &str)
        .collect();
    let args_slice: &'static [&'static str] = Box::leak(args_str.into_boxed_slice());
    let args: Args = facet_args::from_slice(args_slice).map_err(|e| e.into_owned())?;

    log::info!("Args: {}", args.pretty());

    let config = libconfig::load().load_mom_config(&args.mom_config)?;
    let tenant_config = fs_err::tokio::read_to_string(&args.tenant_config).await?;
    log::info!("Tenant config payload: {tenant_config}");
    let tenant_list: Vec<TenantConfig> = serde_json::from_str(&tenant_config)?;
    log::info!("Tenant list: {}", tenant_list.pretty());

    let tenants: HashMap<TenantDomain, TenantInfo> = tenant_list
        .into_iter()
        .map(|mut tc| -> eyre::Result<(TenantDomain, TenantInfo)> {
            log::info!("Processing tenant: {}", tc.name);

            // Derive cookie sauce if not already set and secrets exist
            if let Some(ref mut secrets) = tc.secrets {
                log::info!(
                    "Found secrets for tenant {}. Checking cookie_sauce...",
                    tc.name
                );
                if secrets.cookie_sauce.is_none() {
                    log::info!(
                        "No cookie_sauce set for tenant {}. Deriving from global cookie_sauce...",
                        tc.name
                    );
                    let global_cookie_sauce = &config.secrets.cookie_sauce;
                    let derived_sauce =
                        mom_types::derive_cookie_sauce(global_cookie_sauce, &tc.name);
                    secrets.cookie_sauce = Some(derived_sauce);
                    log::info!("Set derived cookie_sauce for tenant {}.", tc.name);
                } else {
                    log::info!("Tenant {} already has a cookie_sauce set.", tc.name);
                }
            } else if Environment::default() == Environment::Development {
                // In development, create dummy secrets with derived cookie sauce
                log::info!(
                    "No secrets found for tenant {} in development. Creating dev secrets.",
                    tc.name
                );
                let global_cookie_sauce = &config.secrets.cookie_sauce;
                let derived_sauce = mom_types::derive_cookie_sauce(global_cookie_sauce, &tc.name);

                tc.secrets = Some(config_types::TenantSecrets {
                    aws: config_types::AwsSecrets {
                        access_key_id: "dev-access-key".to_string(),
                        secret_access_key: "dev-secret-key".to_string(),
                    },
                    patreon: None,
                    github: None,
                    cookie_sauce: Some(derived_sauce),
                });
                log::info!("Dev secrets created for tenant {}.", tc.name);
            } else {
                // In production, this is an error
                log::info!(
                    "No secrets configured for tenant {} in production. Returning error.",
                    tc.name
                );
                return Err(eyre::eyre!("No secrets configured for tenant {}", tc.name));
            }

            let base_dir = tc.base_dir_for_dev.clone().unwrap_or_else(|| {
                let dir = config.tenant_data_dir.join(tc.name.as_str());
                log::info!(
                    "No base_dir_for_dev set for tenant {}, using default: {}",
                    tc.name,
                    dir
                );
                dir
            });

            log::info!(
                "Setting up TenantInfo for tenant {} with base_dir: {}",
                tc.name,
                base_dir
            );

            Ok((tc.name.clone(), TenantInfo { base_dir, tc }))
        })
        .collect::<eyre::Result<HashMap<_, _>>>()?;

    let port = if let Ok(port_str) = std::env::var("WEB_PORT") {
        port_str.parse::<u16>().unwrap_or(1118)
    } else {
        1118
    };

    let listener = if let Some(socket_fd) = args.socket_fd {
        // Receive the TCP listener via Unix socket
        log::info!("Receiving TCP listener from socket fd {socket_fd}");

        use sendfd::RecvWithFd;
        use std::os::unix::io::{FromRawFd, RawFd};
        use std::os::unix::net::UnixStream;

        // Convert the fd to a UnixStream
        let unix_stream = unsafe { UnixStream::from_raw_fd(socket_fd) };

        // Receive the TCP listener fd
        let mut buf = [0u8; 1];
        let mut fds = [0 as RawFd; 1];
        let (_, fd_count) = unix_stream
            .recv_with_fd(&mut buf, &mut fds)
            .map_err(|e| eyre::eyre!("Failed to receive TCP listener fd: {}", e))?;

        if fd_count == 0 {
            return Err(eyre::eyre!("No file descriptor received"));
        }

        let tcp_fd = fds[0];

        // Convert to std TcpListener
        let std_listener = unsafe { std::net::TcpListener::from_raw_fd(tcp_fd) };

        // Convert to tokio TcpListener
        TcpListener::from_std(std_listener)?
    } else {
        // Fallback to binding directly
        TcpListener::bind(format!("[::]:{port}")).await?
    };

    libmom::load()
        .serve(MomServeArgs {
            config,
            web: WebConfig {
                env: Environment::default(),
                port,
            },
            tenants,
            listener,
        })
        .await
        .map_err(|err| eyre::eyre!(err.to_string()))
}
