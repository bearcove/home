use autotrait::autotrait;
use config_types::WebConfig;
use conflux::{InputPathRef, Media, RevisionView};
use image_types::LogicalPixels;

mod impls;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    &ModImpl
}

pub struct MediaMarkupOpts<'a> {
    pub path: &'a InputPathRef,
    pub media: &'a Media,
    pub rv: &'a dyn RevisionView,

    pub id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub alt: Option<&'a str>,

    // these override the media's original width/height and specify CSS pixel dimensions
    pub width: Option<LogicalPixels>,
    pub height: Option<LogicalPixels>,

    pub class: Option<&'a str>,
    pub web: WebConfig,
}

#[autotrait]
impl Mod for ModImpl {
    /// Generate HTML markup for a `Media`
    fn media_html_markup(&self, opts: MediaMarkupOpts<'_>) -> eyre::Result<String> {
        impls::media_markup(opts)
    }
}
