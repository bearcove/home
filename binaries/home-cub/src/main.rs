use camino::Utf8PathBuf;
use config_types::{
    CubConfigBundle, Environment, MOM_DEV_API_KEY, MomConfig, MomSecrets, WebConfig,
};
use facet::Facet;
use facet_pretty::FacetPretty;
use libcub::OpenBehavior;
use mom_types::MomServeArgs;
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

    let web = WebConfig {
        env,
        port: cub_addr.port(),
    };

    if env.is_dev() {
        // Try to bind to mom on port 1118. If it fails, fall back to random port (0).
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

        let mom_conf = MomConfig {
            tenant_data_dir: Utf8PathBuf::from("/tmp/tenant_data"),
            secrets: MomSecrets {
                readonly_api_key: MOM_DEV_API_KEY.to_owned(),
                scoped_api_keys: Default::default(),
                cookie_sauce: "dev_global_cookie_sauce_secret".to_owned(),
            },
        };

        tokio::spawn(async move {
            if let Err(e) = libmom::load()
                .serve(MomServeArgs {
                    config: mom_conf,
                    web,
                    tenants,
                    listener: mom_ln,
                })
                .await
            {
                eprintln!("\n\n\x1b[31;1m========================================");
                eprintln!("🚨 FATAL ERROR: Mom server died unexpectedly 🚨");
                eprintln!("💀 We're dying! This is why: 💀");
                eprintln!("Error details: {e}");
                eprintln!("🔥 She's taking us down with her! 🔥");
                eprintln!("Please report this to @fasterthanlime ASAP!");
                eprintln!("========================================\x1b[0m\n");
                std::process::exit(1);
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
