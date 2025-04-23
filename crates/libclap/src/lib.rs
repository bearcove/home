use autotrait::autotrait;
use camino::Utf8PathBuf;

use clap::{Parser, Subcommand};

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    /// Parses command line arguments
    fn parse(&self) -> Args {
        Args::parse()
    }
}

#[derive(Parser)]
#[clap(
    name = "home",
    version = "edge",
    author = "Amos Wenger <amos@bearcove.eu>",
    about = "Cozy authoring solution"
)]
pub struct Args {
    #[clap(subcommand)]
    pub sub: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    Doctor(DoctorArgs),
    Serve(ServeArgs),
    Mom(MomArgs),
    Term(TermArgs),
    Init(InitArgs),
}

/// Records a terminal session with colors, ready to paste into markdown
#[derive(Parser)]
pub struct TermArgs {
    /// Enable strict mode
    #[clap(long)]
    pub strict: bool,

    /// Print CSS
    #[clap(long)]
    pub css: bool,

    /// Positional arguments
    #[clap()]
    pub args: Vec<String>,
}

#[derive(Parser, PartialEq, Eq, Debug)]
/// Serves the site
pub struct ServeArgs {
    #[clap(default_value = ".")]
    /// Paths to serve
    pub roots: Vec<Utf8PathBuf>,

    #[clap(long)]
    /// Optional config file
    pub config: Option<Utf8PathBuf>,

    #[clap(long)]
    /// Open the site in the default browser
    pub open: bool,
}

#[derive(Parser, PartialEq, Eq, Debug)]
/// Serves mom
pub struct MomArgs {
    #[clap(long)]
    /// mom config file
    pub mom_config: Utf8PathBuf,

    #[clap(long)]
    /// tenant config file
    pub tenant_config: Utf8PathBuf,
}

#[derive(Parser, PartialEq, Eq, Debug)]
/// Initializes the project
pub struct InitArgs {
    #[clap(default_value = ".")]
    /// directory to initialize
    pub dir: Utf8PathBuf,

    #[clap(long)]
    /// overwrite existing files without asking
    pub force: bool,
}

#[derive(Parser, PartialEq, Eq, Debug)]
/// Verifies that home is packaged correctly
pub struct DoctorArgs {}
