include!(".dylo/spec.rs");
include!(".dylo/support.rs");

use camino::Utf8PathBuf;

#[cfg(feature = "impl")]
use clap::{Parser, Subcommand};

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

#[dylo::export]
impl Mod for ModImpl {
    /// Parses command line arguments
    fn parse(&self) -> Args {
        Args::parse()
    }
}

#[cfg_attr(feature = "impl", derive(Parser))]
#[cfg_attr(
    feature = "impl",
    clap(
        name = "home",
        version = "edge",
        author = "Amos Wenger <amos@bearcove.eu>",
        about = "Cozy authoring solution"
    )
)]
pub struct Args {
    #[cfg_attr(feature = "impl", clap(subcommand))]
    pub sub: Cmd,
}

#[cfg_attr(feature = "impl", derive(Subcommand))]
pub enum Cmd {
    Doctor(DoctorArgs),
    Serve(ServeArgs),
    Mom(MomArgs),
    Term(TermArgs),
    Init(InitArgs),
}

/// Records a terminal session with colors, ready to paste into markdown
#[cfg_attr(feature = "impl", derive(Parser))]
pub struct TermArgs {
    /// Enable strict mode
    #[cfg_attr(feature = "impl", clap(long))]
    pub strict: bool,

    /// Print CSS
    #[cfg_attr(feature = "impl", clap(long))]
    pub css: bool,

    /// Positional arguments
    #[cfg_attr(feature = "impl", clap())]
    pub args: Vec<String>,
}

#[cfg_attr(feature = "impl", derive(Parser))]
#[derive(PartialEq, Eq, Debug)]
/// Serves the site
pub struct ServeArgs {
    #[cfg_attr(feature = "impl", clap(default_value = "."))]
    /// Paths to serve
    pub roots: Vec<Utf8PathBuf>,

    #[cfg_attr(feature = "impl", clap(long))]
    /// Optional config file
    pub config: Option<Utf8PathBuf>,

    #[cfg_attr(feature = "impl", clap(long))]
    /// Open the site in the default browser
    pub open: bool,
}

#[cfg_attr(feature = "impl", derive(Parser))]
#[derive(PartialEq, Eq, Debug)]
/// Serves mom
pub struct MomArgs {
    #[cfg_attr(feature = "impl", clap(long))]
    /// mom config file
    pub mom_config: Utf8PathBuf,

    #[cfg_attr(feature = "impl", clap(long))]
    /// tenant config file
    pub tenant_config: Utf8PathBuf,
}

#[cfg_attr(feature = "impl", derive(Parser))]
#[derive(PartialEq, Eq, Debug)]
/// Initializes the project
pub struct InitArgs {
    #[cfg_attr(feature = "impl", clap(default_value = "."))]
    /// directory to initialize
    pub dir: Utf8PathBuf,

    #[cfg_attr(feature = "impl", clap(long))]
    /// overwrite existing files without asking
    pub force: bool,
}

#[cfg_attr(feature = "impl", derive(Parser))]
#[derive(PartialEq, Eq, Debug)]
/// Verifies that home is packaged correctly
pub struct DoctorArgs {}
