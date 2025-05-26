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

    assert_eq!(
        Environment::default(),
        Environment::Production,
        "mom subcommand is only for production right now"
    );

    let config = libconfig::load().load_mom_config(&args.mom_config)?;
    let tenant_config = fs_err::tokio::read_to_string(&args.tenant_config).await?;
    let tenant_list: Vec<TenantConfig> =
        facet_json::from_str(&tenant_config).map_err(|e| e.into_owned())?;
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
