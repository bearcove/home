use crate::{DerivationKind, InputPath};

/// A derived asset, e.g. "jxl" => "jxl.png"
/// or "drawio" => "drawio.svg"
/// or "mp4" => "vp9+opus.webm"
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Derivation {
    // the main source of truth for this mixed asset — found
    // in the tenant's object store, or on disk in development
    pub input: InputPath,

    pub kind: DerivationKind,
}
