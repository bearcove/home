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
    let _sentry_guard = sentrywrap::install();

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

                // Check for git credentials in environment variables
                let git_credentials = match (
                    std::env::var("HOME_GIT_USERNAME"),
                    std::env::var("HOME_GIT_PASSWORD")
                ) {
                    (Ok(username), Ok(password)) => {
                        log::info!("Found git credentials in environment variables for tenant {}", tc.name);
                        Some(config_types::GitCredentials { username, password })
                    }
                    _ => {
                        log::info!("No git credentials found in environment variables (HOME_GIT_USERNAME, HOME_GIT_PASSWORD) for tenant {}", tc.name);
                        None
                    }
                };

                // Check for Patreon secrets in environment variables
                let patreon_secrets = match (
                    std::env::var("HOME_PATREON_OAUTH_CLIENT_ID"),
                    std::env::var("HOME_PATREON_OAUTH_CLIENT_SECRET")
                ) {
                    (Ok(client_id), Ok(client_secret)) => {
                        log::info!("Found Patreon secrets in environment variables for tenant {}", tc.name);
                        Some(config_types::PatreonSecrets {
                            oauth_client_id: client_id,
                            oauth_client_secret: client_secret
                        })
                    }
                    _ => {
                        log::info!("No Patreon secrets found in environment variables (HOME_PATREON_OAUTH_CLIENT_ID, HOME_PATREON_OAUTH_CLIENT_SECRET) for tenant {}", tc.name);
                        None
                    }
                };

                // Check for GitHub secrets in environment variables
                let github_secrets = match (
                    std::env::var("HOME_GITHUB_OAUTH_CLIENT_ID"),
                    std::env::var("HOME_GITHUB_OAUTH_CLIENT_SECRET")
                ) {
                    (Ok(client_id), Ok(client_secret)) => {
                        log::info!("Found GitHub secrets in environment variables for tenant {}", tc.name);
                        Some(config_types::GithubSecrets {
                            oauth_client_id: client_id,
                            oauth_client_secret: client_secret
                        })
                    }
                    _ => {
                        log::info!("No GitHub secrets found in environment variables (HOME_GITHUB_OAUTH_CLIENT_ID, HOME_GITHUB_OAUTH_CLIENT_SECRET) for tenant {}", tc.name);
                        None
                    }
                };

                // Check for Discord secrets in environment variables
                let discord_secrets = match (
                    std::env::var("HOME_DISCORD_OAUTH_CLIENT_ID"),
                    std::env::var("HOME_DISCORD_OAUTH_CLIENT_SECRET")
                ) {
                    (Ok(client_id), Ok(client_secret)) => {
                        log::info!("Found Discord secrets in environment variables for tenant {}", tc.name);
                        Some(config_types::DiscordSecrets {
                            oauth_client_id: client_id,
                            oauth_client_secret: client_secret
                        })
                    }
                    _ => {
                        log::info!("No Discord secrets found in environment variables (HOME_DISCORD_OAUTH_CLIENT_ID, HOME_DISCORD_OAUTH_CLIENT_SECRET) for tenant {}", tc.name);
                        None
                    }
                };

                tc.secrets = Some(config_types::TenantSecrets {
                    aws: config_types::AwsSecrets {
                        access_key_id: "dev-access-key".to_string(),
                        secret_access_key: "dev-secret-key".to_string(),
                    },
                    patreon: patreon_secrets,
                    github: github_secrets,
                    discord: discord_secrets,
                    stripe: None,
                    git: git_credentials,
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

            let base_dir = match tc.base_dir_for_dev.clone() {
                Some(base_dir_for_dev) => {
                    base_dir_for_dev
                },
                None => {
                    let dir = config.tenant_data_dir.join(tc.name.as_str());
                    log::info!(
                        "No base_dir_for_dev set for tenant {}, using default: {}",
                        tc.name,
                        dir
                    );
                    dir
                }
            };

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
