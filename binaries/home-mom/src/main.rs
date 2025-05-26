use camino::Utf8PathBuf;
use facet::Facet;
use facet_pretty::FacetPretty;
use skelly::{eyre, log};

#[derive(Facet)]
struct Args {
    #[facet(long)]
    /// mom config file
    pub mom_config: Utf8PathBuf,

    #[facet(long)]
    /// tenant config file
    pub tenant_config: Utf8PathBuf,
}

fn main() -> eyre::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<String>>();
    let args_str: Vec<&'static str> = args
        .into_iter()
        .map(|s| Box::leak(s.into_boxed_str()) as &str)
        .collect();
    let args_slice: &'static [&'static str] = Box::leak(args_str.into_boxed_slice());
    let args: Args = facet_args::from_slice(args_slice).map_err(|e| e.into_owned())?;

    log::info!("Args: {}", args.pretty());

    Ok(())
}
