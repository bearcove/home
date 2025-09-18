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

    let args: Args = facet_args::from_std_args()?;

    dev_setup::init_project(&args.dir, args.force)
        .await
        .map_err(|err| eyre::eyre!(err.to_string()))
}
