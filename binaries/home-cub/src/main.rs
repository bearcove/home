use camino::Utf8PathBuf;
use config_types::{
    CubConfigBundle, Environment, MOM_DEV_API_KEY, MomConfig, MomSecrets, TenantConfig, WebConfig,
};
use facet::Facet;
use facet_pretty::FacetPretty;
use libcub::OpenBehavior;
use skelly::{
    eyre::{self, Context},
    log,
    owo_colors::OwoColorize,
};
use tokio::net::TcpListener;

#[derive(Facet)]
struct Args {
    #[facet(positional, default = ".".into())]
    /// Paths to serve
    pub roots: String,

    #[facet(long, default)]
    /// Optional config file
    pub config: Option<Utf8PathBuf>,

    #[facet(long, default)]
    /// Open the site in the default browser
    pub open: bool,
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

    let CubConfigBundle { mut cc, tenants } = libconfig::load()
        .load_cub_config(
            args.config.as_ref().map(|p| p.as_path()),
            args.roots
                .split(',')
                .map(Utf8PathBuf::from)
                .collect::<Vec<Utf8PathBuf>>(),
        )
        .wrap_err("while reading cub config")?;

    let env = Environment::default();
    log::info!("Booting up in {env}");

    let addr = cc.address;
    let cub_ln;

    // Try to bind exactly as specified in cc.address first.
    match TcpListener::bind(&addr).await {
        Ok(listener) => {
            let ln_addr = listener.local_addr().unwrap();
            cc.address = ln_addr;
            cub_ln = listener;
        }
        Err(e) => {
            // If random port fallback is NOT allowed, error and exit.
            if !cc.random_port_fallback {
                return Err(eyre::eyre!(
                    "Failed to bind to address {addr}: {e}\nRandom port fallback is disabled (cc.random_port_fallback == false), so exiting."
                ));
            }
            // Otherwise, bind to any available port (port 0)
            let mut random_addr = addr;
            random_addr.set_port(0);
            let listener = tokio::net::TcpListener::bind(&random_addr).await.map_err(|e| {
                eyre::eyre!(
                    "Failed to bind to random port (fallback after failing to bind to {addr}): {e}"
                )
            })?;
            let ln_addr = listener.local_addr().unwrap();
            log::info!(
                "Random port {} assigned by OS after failing to bind to {}",
                ln_addr.port(),
                addr
            );
            cc.address = ln_addr;
            cub_ln = listener;
        }
    }
    let cub_addr = cub_ln.local_addr()?;

    let _web = WebConfig {
        env,
        port: cub_addr.port(),
    };

    if env.is_dev() {
        // Create a temporary directory for mom config files
        let temp_dir = std::env::temp_dir().join(format!("home-cub-mom-{}", std::process::id()));
        fs_err::tokio::create_dir_all(&temp_dir).await?;

        // Create mom config
        let mom_conf = MomConfig {
            tenant_data_dir: Utf8PathBuf::from("/tmp/tenant_data"),
            secrets: MomSecrets {
                readonly_api_key: MOM_DEV_API_KEY.to_owned(),
                scoped_api_keys: Default::default(),
                cookie_sauce: "dev_global_cookie_sauce_secret".to_owned(),
            },
        };

        // Write mom config to temp file
        let mom_config_path = temp_dir.join("mom-config.json");
        let mom_config_json = facet_json::to_string(&mom_conf);
        fs_err::tokio::write(&mom_config_path, mom_config_json).await?;

        // Convert tenants HashMap to Vec<TenantConfig> for serialization
        let tenant_list: Vec<TenantConfig> = tenants
            .values()
            .map(|ti| {
                let mut tc = ti.tc.clone();
                tc.base_dir_for_dev = Some(ti.base_dir.clone());
                tc
            })
            .collect();

        // Write tenant config to temp file
        let tenant_config_path = temp_dir.join("tenant-config.json");
        let tenant_config_json = facet_json::to_string(&tenant_list);
        fs_err::tokio::write(&tenant_config_path, tenant_config_json).await?;

        // Create the TCP listener for mom
        let mom_ln = match TcpListener::bind("127.0.0.1:1118").await {
            Ok(ln) => ln,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to bind mom to 127.0.0.1:1118: {e}\nFalling back to a random port (0) for mom."
                );
                TcpListener::bind("127.0.0.1:0").await?
            }
        };
        let mom_addr = mom_ln.local_addr()?;
        eprintln!("Mom is listening on {}", mom_addr.blue());
        cc.mom_base_url = format!("http://{mom_addr}");

        // Create a Unix socket pair for passing the TCP listener
        use sendfd::SendWithFd;
        use std::os::unix::io::AsRawFd;
        use std::os::unix::net::UnixStream;

        let (parent_sock, child_sock) = UnixStream::pair()?;

        // Find the home-mom binary
        let current_exe = std::env::current_exe()?;
        let exe_dir = current_exe
            .parent()
            .ok_or_else(|| eyre::eyre!("Failed to get exe dir"))?;
        let mom_exe = exe_dir.join("home-mom");

        if !mom_exe.exists() {
            return Err(eyre::eyre!("home-mom binary not found at {:?}", mom_exe));
        }

        // Spawn home-mom process using skelly::spawn
        let mut cmd = tokio::process::Command::new(&mom_exe);
        cmd.arg("--mom-config")
            .arg(&mom_config_path)
            .arg("--tenant-config")
            .arg(&tenant_config_path)
            .env("HOME_ENV", "development")
            .env("WEB_PORT", mom_addr.port().to_string());

        // Duplicate the child socket fd to pass it safely
        let child_fd = child_sock.as_raw_fd();
        let dup_fd = unsafe { libc::dup(child_fd) };
        if dup_fd < 0 {
            return Err(eyre::eyre!("Failed to duplicate socket fd"));
        }
        cmd.arg("--socket-fd").arg(dup_fd.to_string());

        // Set close-on-exec for the original fd but not the duplicate
        unsafe {
            libc::fcntl(child_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }

        let mut cmd = skelly::spawn(cmd);

        let mut child = cmd.spawn()?;

        // Send the TCP listener file descriptor through the Unix socket
        let tcp_fd = mom_ln.as_raw_fd();
        parent_sock
            .send_with_fd(&[1], &[tcp_fd])
            .map_err(|e| eyre::eyre!("Failed to send TCP listener fd: {}", e))?;

        // Don't close the listener - it needs to stay open
        std::mem::forget(mom_ln);

        // Clean up temp directory on exit
        let temp_dir_clone = temp_dir.clone();
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    if !status.success() {
                        eprintln!("\n\n\x1b[31;1m========================================");
                        eprintln!("🚨 FATAL ERROR: Mom server died unexpectedly 🚨");
                        eprintln!("💀 We're dying! This is why: 💀");
                        eprintln!("Exit status: {status}");
                        eprintln!("🔥 She's taking us down with her! 🔥");
                        eprintln!("Please report this to @fasterthanlime ASAP!");
                        eprintln!("========================================\x1b[0m\n");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to wait for mom process: {e}");
                    std::process::exit(1);
                }
            }

            // Clean up temp directory
            if let Err(e) = fs_err::tokio::remove_dir_all(&temp_dir_clone).await {
                log::warn!("Failed to clean up temp directory: {e}");
            }
        });
    }

    eprintln!(
        "Starting up cub, who expects a mom at: {}",
        cc.mom_base_url.blue()
    );
    libcub::load()
        .serve(
            cc,
            cub_ln,
            if args.open {
                OpenBehavior::OpenOnStart
            } else {
                OpenBehavior::DontOpen
            },
        )
        .await
        .map_err(|err| eyre::eyre!(err.to_string()))
}
