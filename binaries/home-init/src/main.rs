use camino::Utf8PathBuf;
use facet::Facet;
use skelly::eyre;

mod dev_setup;

#[derive(Facet)]
/// Initializes the project
pub struct Args {
    #[facet(default = ".".into())]
    /// directory to initialize
    pub dir: Utf8PathBuf,

    #[facet(long, default = false)]
    /// overwrite existing files without asking
    pub force: bool,
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

    dev_setup::init_project(&args.dir, args.force)
        .await
        .map_err(|err| eyre::eyre!(err.to_string()))
}
