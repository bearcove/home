use std::collections::HashMap;

use camino::Utf8PathBuf;
use config_types::{
    CubConfigBundle, Environment, MOM_DEV_API_KEY, MomConfig, MomSecrets, TenantConfig,
    TenantDomain, TenantInfo, WebConfig,
};
use eyre::Context;
use libc as _;
use libcub::OpenBehavior;

use libclap::Cmd;
use mom_types::MomServeArgs;
use owo_colors::OwoColorize;
use tokio::net::TcpListener;
use tracing::info;

mod dev_setup;

pub(crate) fn print_error(e: &eyre::Report) {
    print_error_to_writer(e, &mut std::io::stderr());
}

pub(crate) fn print_error_to_writer(e: &eyre::Report, writer: &mut impl std::io::Write) {
    for (i, e) in e.chain().enumerate() {
        writeln!(writer, "{}. {}", i + 1, e).unwrap();
    }

    if let Some(bt) = liberrhandling::load().format_backtrace_to_terminal_colors(e) {
        writeln!(writer, "{bt}").unwrap();
    }
}

#[tokio::main(flavor = "multi_thread")]
pub async fn main() -> eyre::Result<()> {
    real_main().await
}

async fn real_main() -> eyre::Result<()> {
    liberrhandling::load().install();
    libtracingsub::load().install();

    let args = libclap::load().parse();

    let res = match args.sub {
        Cmd::Doctor(_) => {
            libdoctor::load().run().await;
            Ok(())
        }
        Cmd::Init(args) => dev_setup::init_project(&args.dir, args.force)
            .await
            .map_err(|err| eyre::eyre!(err.to_string())),
        Cmd::Serve(args) => {
            let CubConfigBundle { mut cc, tenants } = libconfig::load()
                .load_cub_config(args.config.as_ref().map(|p| p.as_path()), args.roots)
                .wrap_err("while reading cub config")?;

            let env = Environment::default();
            eprintln!("Booting up in {env}");

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
                    info!(
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
                cc.mom_base_url = format!("http://{}", mom_addr);

                let mom_conf = MomConfig {
                    tenant_data_dir: Utf8PathBuf::from("/tmp/tenant_data"),
                    secrets: MomSecrets {
                        readonly_api_key: MOM_DEV_API_KEY.to_owned(),
                        scoped_api_keys: Default::default(),
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
                        eprintln!("Error details: {}", e);
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
        Cmd::Mom(args) => {
            assert_eq!(
                Environment::default(),
                Environment::Production,
                "mom subcommand is only for production right now"
            );

            let config = libconfig::load().load_mom_config(&args.mom_config)?;
            let tenant_list: Vec<TenantConfig> =
                serde_json::from_str(&tokio::fs::read_to_string(&args.tenant_config).await?)?;
            let tenants: HashMap<TenantDomain, TenantInfo> = tenant_list
                .into_iter()
                .map(|tc| {
                    (
                        tc.name.clone(),
                        TenantInfo {
                            base_dir: config.tenant_data_dir.join(tc.name.as_str()),
                            tc,
                        },
                    )
                })
                .collect();

            let listener = TcpListener::bind("[::]:1118").await?;

            libmom::load()
                .serve(MomServeArgs {
                    config,
                    web: WebConfig {
                        env: Environment::Production,
                        port: 999, // doesn't matter in prod — it's not used
                    },
                    tenants,
                    listener,
                })
                .await
                .map_err(|err| eyre::eyre!(err.to_string()))
        }
        Cmd::Term(args) => {
            libterm::load().run(args);
            Ok(())
        }
    };

    match res {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Fatal error, we'll quit soon-ish. Before we do, here's the error!");
            print_error(&e);

            eprintln!("☠️☠️☠️");
            eprintln!("\x1b[31mWE ARE NOT SERVING THE WEBSITE :( SOMETHING FAILED\x1b[0m");
            eprintln!("☠️☠️☠️\n\n");

            std::process::exit(1);
        }
    }

    Ok(())
}
