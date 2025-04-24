//! Conflux is where all crates meet: types and interfaces that are shared.type
//! between crates.

use core::fmt;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

use bytes::Bytes;
use camino::{Utf8Path, Utf8PathBuf};
use content_type::ContentType;
use credentials::UserInfo;
use image::{ICodec, IntrinsicPixels};
use merde::{CowStr, time::Rfc3339};
use objectstore::{ObjectStoreKey, input_key};
pub use time::OffsetDateTime;

use closest::{GetOrHelp, ResourceKind};
use config::{FontStyle, FontWeight, RevisionConfig, TenantConfig, TenantInfo, WebConfig};
use plait::plait;

mod av;
pub use av::*;

mod derivations;
pub use derivations::*;

/// An error that occurred while loading a revision
#[derive(Debug, Clone)]
pub struct RevisionError(pub String);

merde::derive!(impl (Serialize, Deserialize) for struct RevisionError transparent);

impl std::fmt::Display for RevisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RevisionError {}

/// Gets access to various bits of useful info about revisions.
/// Used from templates.
pub trait RevisionView: Send + Sync + 'static {
    fn rev(&self) -> Result<&Revision, RevisionError> {
        unimplemented!()
    }

    fn cachebuster(&self) -> &dyn CacheBuster {
        unimplemented!()
    }
}

impl RevisionView for Revision {
    fn rev(&self) -> Result<&Revision, RevisionError> {
        Ok(self)
    }

    fn cachebuster(&self) -> &dyn CacheBuster {
        self
    }
}

pub use eyre::Result;

pub trait CacheBuster: Send + Sync + 'static {
    /// Cache-bust the given asset path.
    /// The asset path might start with a slash, even though this is all relative to `${base_dir}`.
    /// e.g. `/content/img/logo-square.png` might bust to ``
    fn asset_url(&self, web: WebConfig, asset_path: &InputPathRef) -> Result<AbsoluteUrl> {
        let _ = (web, asset_path, self);
        unimplemented!()
    }

    /// Get the media for the given input path
    fn media(&self, path: &InputPathRef) -> Result<&Media> {
        let _ = (path, self);
        unimplemented!()
    }
}

impl RevisionView for () {
    fn cachebuster(&self) -> &dyn CacheBuster {
        self
    }
}

impl CacheBuster for () {}

impl CacheBuster for Revision {
    fn asset_url(&self, web: WebConfig, asset_path: &InputPathRef) -> Result<AbsoluteUrl> {
        let rev = self;

        let asset_route = rev
            .asset_routes
            .get_or_help(ResourceKind::AssetRoute, asset_path)?;

        Ok(asset_route.to_cdn_url_string(&rev.ti.tc, web))
    }

    fn media(&self, path: &InputPathRef) -> Result<&Media> {
        let rev = self.rev()?;
        rev.media.get_or_help(ResourceKind::Media, path)
    }
}

pub type Toc = Vec<TocEntry>;

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TocEntry {
    // 1 through 6
    pub level: u8,
    // something like "The basics"
    pub text: String,
    // something like "the-basics"
    pub slug: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PageKind {
    // e.g. `/episodes`
    EpisodesListing,
    // e.g. `/episodes/dma`
    Episode,
    // e.g. `/articles`
    ArticleListing,
    // e.g. `/articles/10-months-of-itch`
    Article,
    // e.g. `/series`
    SeriesListing,
    // e.g. `/series/making-our-own-ping`
    SeriesIndex,
    // e.g. `/series/making-our-own-ping/part-1`
    SeriesPart,
    // e.g. `/tests/blah`
    Test,
    // e.g. `/about`
    Other,
}

impl From<&RouteRef> for PageKind {
    fn from(path: &RouteRef) -> Self {
        let tokens: Vec<&str> = path.as_str().split('/').skip(1).collect();
        match tokens.as_slice() {
            ["episodes"] => PageKind::EpisodesListing,
            ["episodes", _slug] => PageKind::Episode,
            ["articles"] => PageKind::ArticleListing,
            ["articles", _slug] => PageKind::Article,
            ["series"] => PageKind::SeriesListing,
            ["series", _slug] => PageKind::SeriesIndex,
            ["series", _slug, _part] => PageKind::SeriesPart,
            ["tests", _slug] => PageKind::Test,
            _ => PageKind::Other,
        }
    }
}

#[test]
fn pagekind_from_path() {
    assert_eq!(
        PageKind::from(RouteRef::from_str("/articles")),
        PageKind::ArticleListing
    );
    assert_eq!(
        PageKind::from(RouteRef::from_str("/articles/10-months-of-itch")),
        PageKind::Article
    );
    assert_eq!(
        PageKind::from(RouteRef::from_str("/series")),
        PageKind::SeriesListing
    );
    assert_eq!(
        PageKind::from(RouteRef::from_str("/series/making-our-own-ping")),
        PageKind::SeriesIndex
    );
    assert_eq!(
        PageKind::from(RouteRef::from_str("/series/making-our-own-ping/part-1")),
        PageKind::SeriesPart
    );
    assert_eq!(
        PageKind::from(Route::new(String::from("/about")).as_ref()),
        PageKind::Other
    );
    assert_eq!(PageKind::from(RouteRef::from_str("/")), PageKind::Other);
    assert_eq!(
        PageKind::from(RouteRef::from_str("/episodes")),
        PageKind::EpisodesListing
    );
    assert_eq!(
        PageKind::from(RouteRef::from_str("/episodes/dma")),
        PageKind::Episode
    );
    assert_eq!(
        PageKind::from(RouteRef::from_str("/tests/example")),
        PageKind::Test
    );
}

#[derive(Clone)]
pub struct LoadedPage {
    /// the tenant this page belongs to
    pub ti: Arc<TenantInfo>,

    /// the web config we've been loaded with
    pub web: WebConfig,

    /// input path of the page
    pub path: InputPath,

    /// the canonical route path of the page — there might be aliases, but this is the canonical one
    pub route: Route,

    /// kind of page
    pub kind: PageKind,

    /// plain text version of the page
    pub plain_text: String,

    /// HTML version of the page
    pub html: String,

    /// estimated reading time
    pub reading_time: i64,

    /// table of contents
    pub toc: Toc,

    /// crates talked about (and which version)
    pub crates: HashMap<String, CrateVersion>,

    /// github repos talked about
    pub github_repos: Vec<String>,

    /// links to other pages, either relative (`/articles/10-months-of-itch`)
    /// or absolute (`https://fasterthanli.me/articles/10-months-of-itch`)
    pub links: Vec<Href>,

    // frontmatter
    pub title: String,
    pub template: String,
    pub date: Rfc3339<OffsetDateTime>,
    pub draft: bool,
    pub archive: bool,
    pub aliases: Vec<Route>,
    pub tags: Vec<String>,
    pub ongoing: bool,

    pub draft_code: Option<String>,

    pub updated_at: Option<Rfc3339<OffsetDateTime>>,
    pub rust_version: Option<String>,

    // if this page is a series part, then here's the info about
    // the series it's part of.
    pub series_link: Option<SeriesLink>,

    // if this page is a series index, then here's the info about
    // its parts.
    pub parts: Vec<Part>,

    // input paths of all the direct children of this page
    pub children: Vec<InputPath>,

    pub show_patreon_credits: bool,
    pub hide_patreon_plug: bool,
    pub hide_comments: bool,
    pub hide_metadata: bool,

    pub video_info: VideoInfo,

    // media info of `path/to/page/_index.md/../_thumb.jxl`, if it exists
    // (ie. `path/to/page/_thumb.jxl`)
    pub thumb: Option<PageThumb>,

    // media info of the parent's thumb, if they have one
    pub parent_thumb: Option<PageThumb>,
}

/// The thumbnail for a page (if it exists)
#[derive(Debug, Clone)]
pub struct PageThumb {
    pub path: InputPath,
    pub media: Media,
}

/// Determines what kind of access someone has to articles etc.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Viewer {
    /// User ID matches admin in config
    pub is_admin: bool,

    /// As of Nov 2022, 5EUR/month or above
    pub has_bronze: bool,

    /// As of Nov 2022, 10EUR/month or above
    pub has_silver: bool,
}

merde::derive!(
    impl (Serialize, Deserialize) for struct Viewer { is_admin, has_bronze, has_silver }
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessOverride {
    AdminMeansBronze,
    AdminMeansSilver,
}

impl AccessOverride {
    /// Determines what kind of access override is applied based on the query string.
    /// Recognizes "admin_means_bronze" and "admin_means_silver".
    pub fn from_raw_query(raw_query: &str) -> Option<Self> {
        if raw_query.contains("admin_means_bronze") {
            Some(Self::AdminMeansBronze)
        } else if raw_query.contains("admin_means_silver") {
            Some(Self::AdminMeansSilver)
        } else {
            None
        }
    }
}

impl Viewer {
    /// Anonymous access
    pub fn anon() -> Self {
        Self {
            is_admin: false,
            has_bronze: false,
            has_silver: false,
        }
    }

    /// Follow the tier from the user info
    pub fn new(
        rc: RevisionConfig,
        user_info: Option<&UserInfo<'_>>,
        access_override: Option<AccessOverride>,
    ) -> Self {
        let mut v = Self::anon();
        if let Some(user_info) = user_info {
            if let Some(github_id) = user_info.profile.github_id.as_deref() {
                if rc.admin_github_ids.iter().any(|id| id == github_id) {
                    v.is_admin = true;
                }
            }
            if let Some(patreon_id) = user_info.profile.patreon_id.as_deref() {
                if rc.admin_patreon_ids.iter().any(|id| id == patreon_id) {
                    v.is_admin = true;
                }
            }

            if let Some(tier) = &user_info.tier {
                match tier.title.as_ref() {
                    "Bronze" => {
                        v.has_bronze = true;
                    }
                    "Silver" | "Gold" | "Creator" => {
                        v.has_bronze = true;
                        v.has_silver = true;
                    }
                    _ => {}
                }
            }
        }

        if let Some(access_override) = access_override {
            match access_override {
                AccessOverride::AdminMeansBronze => {
                    v.has_bronze = v.is_admin;
                }
                AccessOverride::AdminMeansSilver => {
                    v.has_silver = v.is_admin;
                    v.has_bronze = v.is_admin; // silver implies bronze
                }
            }
        }

        v
    }
}

impl LoadedPage {
    #[inline]
    pub fn is_article(&self) -> bool {
        matches!(self.kind, PageKind::Article)
    }

    #[inline]
    pub fn is_series_index(&self) -> bool {
        matches!(self.kind, PageKind::SeriesIndex)
    }

    #[inline]
    pub fn is_series_part(&self) -> bool {
        matches!(self.kind, PageKind::SeriesPart)
    }

    #[inline]
    pub fn is_series_listing(&self) -> bool {
        matches!(self.kind, PageKind::SeriesListing)
    }

    #[inline]
    pub fn is_article_listing(&self) -> bool {
        matches!(self.kind, PageKind::ArticleListing)
    }

    #[inline]
    pub fn is_indexed(&self) -> bool {
        if self.archive {
            return false;
        }

        matches!(
            self.kind,
            PageKind::Article | PageKind::SeriesIndex | PageKind::SeriesPart
        )
    }

    #[inline]
    pub fn is_visible(&self, viewer: &Viewer) -> bool {
        if self.draft && !viewer.is_admin {
            return false;
        }
        if self.date.0 > OffsetDateTime::now_utc() && !viewer.is_admin {
            return false;
        }
        match self.kind {
            PageKind::Article
            | PageKind::Episode
            | PageKind::SeriesIndex
            | PageKind::SeriesPart => {
                // okay
            }
            _ => {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn is_listed(&self, viewer: &Viewer) -> bool {
        self.is_visible(viewer) && !self.archive
    }

    pub fn canonical_url(&self, web: WebConfig) -> AbsoluteUrl {
        self.route.to_web_url_string(&self.ti.tc, web)
    }
}

impl PartialEq for LoadedPage {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for LoadedPage {}

impl Hash for LoadedPage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VideoInfo {
    pub dual_feature: bool,
    pub tube: Option<String>,
    pub youtube: Option<String>,
    pub duration: Option<u64>,
}

impl fmt::Debug for LoadedPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPage")
            .field("path", &self.path)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SeriesLink {
    /// the route of the series index page
    pub index_route: Route,

    /// 1-based index: part 1, part 2, etc.
    pub part_number: PartNumber,
}

impl InputPathRef {
    // explodes "foo/bar/baz.png" into ("foo/bar/baz", "png")
    pub fn explode(&self) -> (&str, &str) {
        let (base, ext) = self
            .as_str()
            .rsplit_once('.')
            .unwrap_or((self.as_str(), ""));
        (base, ext)
    }
}

#[derive(Debug)]
pub struct FromDiskPathError {
    path: Utf8PathBuf,
    base_dir: Utf8PathBuf,
}

impl std::fmt::Display for FromDiskPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "disk path {path:?} is not under base dir {base_dir}",
            path = self.path,
            base_dir = self.base_dir
        )
    }
}

impl std::error::Error for FromDiskPathError {}

impl RouteRef {
    /// Given '/foo/bar/baz', returns '/foo/bar'
    pub fn parent(&self) -> Option<&RouteRef> {
        let path = self.as_str();
        if path == "/" {
            return None;
        }
        path.rsplit_once('/').map(|(parent, _)| {
            if parent.is_empty() {
                RouteRef::from_str("/")
            } else {
                RouteRef::from_str(parent)
            }
        })
    }
}

#[test]
fn test_routepath_parent() {
    assert_eq!(
        RouteRef::from_str("/foo/bar/baz").parent(),
        Some(RouteRef::from_str("/foo/bar"))
    );
    assert_eq!(
        RouteRef::from_str("/foo/bar").parent(),
        Some(RouteRef::from_str("/foo"))
    );
    assert_eq!(
        RouteRef::from_str("/foo").parent(),
        Some(RouteRef::from_str("/"))
    );
    assert_eq!(RouteRef::from_str("/").parent(), None);
}

impl InputPathRef {
    /// Transforms the path by removing prefixes, suffixes, and extensions.
    /// E.g. "/content/_index.md" -> "/"
    /// E.g. "/content/about.md" -> "/about"
    /// E.g. "/content/articles/hello-world.md" -> "/articles/hello-world"
    /// E.g. "/content/series/rust-101/part-1/_index.md" -> "/series/rust-101/part-1"
    pub fn to_route_path(&self) -> &RouteRef {
        // Remove leading "/content" if present
        let route_path = self.as_str().trim_start_matches("/content");

        if route_path == "/_index.md" {
            // special case
            return RouteRef::from_str("/");
        }

        // Remove trailing "/_index.md" if present
        let route_path = route_path.trim_end_matches("/_index.md");

        // Remove file extension (everything after the last dot)
        let route_path = match route_path.rfind('.') {
            Some(index) => &route_path[..index],
            None => route_path,
        };

        // Convert the resulting string to a RoutePathRef
        RouteRef::from_str(route_path)
    }
}

#[test]
fn test_to_route_path() {
    assert_eq!(
        InputPathRef::from_str("/content/_index.md").to_route_path(),
        RouteRef::from_str("/")
    );
    assert_eq!(
        InputPathRef::from_str("/content/about.md").to_route_path(),
        RouteRef::from_str("/about")
    );
    assert_eq!(
        InputPathRef::from_str("/content/articles/hello-world.md").to_route_path(),
        RouteRef::from_str("/articles/hello-world")
    );
    assert_eq!(
        InputPathRef::from_str("/content/series/rust-101/part-1/_index.md").to_route_path(),
        RouteRef::from_str("/series/rust-101/part-1")
    );
}

plait! {
    with crates {
        #[cfg(feature = "serde")]
        serde

        #[cfg(feature = "rusqlite")]
        rusqlite

        #[cfg(feature = "minijinja")]
        minijinja

        merde
    }

    /// A route, e.g. `/articles/10-months-of-itch`,
    /// or `/articles/10-months-of-itch/asssets/blah~hash.png`
    /// These always have a leading `/`
    pub struct Route => &RouteRef;

    /// Markdown markup, e.g. `# Hello World`
    pub struct Markdown => &MarkdownRef;

    /// HTML markup, e.g. `<h1>Hello World</h1>`
    pub struct Html => &HtmlRef;

    /// An absolute URL, e.g. `https://fasterthanli.me/articles/10-months-of-itch`
    /// or `https://cdn.fasterthanli.me/articles/10-months-of-itch/asssets/blah~hash.png`
    pub struct AbsoluteUrl => &AbsoluteUrlRef;

    /// The href of a link, e.g. `/articles/10-months-of-itch`
    /// or `https://en.wikipedia.org/wiki/Itch`
    pub struct Href => &HrefRef;

    /// A path to an input file, e.g. `/content/articles/10-months-of-itch/_index.md`,
    /// or `/content/sass/main.scss`. These always have a leading `/`.
    pub struct InputPath => &InputPathRef;

    /// The hash of a pipeline (indicating code changes)
    pub struct PipelineHash => &PipelineHashRef;

    /// The hash of an input file, e.g. `articles/10-months-of-itch/_index.md`
    pub struct InputHash => &InputHashRef;

    /// A hash of all the input hashes that go into building an asset:
    /// itself, its dependencies, the version of the code / processor
    /// used to build it, etc.
    pub struct DerivationHash => &DerivationHashRef;

    /// A crate version, e.g. `0.1.0`
    pub struct CrateVersion => &CrateVersionRef;

    /// A revision ID, e.g. `rev_12345678`
    pub struct RevisionId => &RevisionIdRef;

    /// An ffmpeg codec name, e.g. `h264`, `vp9`, `av1`, `aac`, `opus`
    pub struct FfmpegCodec => &FfmpegCodecRef;

    /// Something like `stereo`, `5.1`, `7.1`
    pub struct FfmpegChannels => &FfmpegChannelsRef;

    /// Something like `yuv420p`, `yuv444p` etc.
    pub struct FfmpegPixelFormat => &FfmpegPixelFormatRef;

    /// A 'codec' parameter for the that can be used in the "codecs" parameter of a a content-type
    /// (and thus, a source tag)
    ///
    /// cf. <https://developer.mozilla.org/en-US/docs/Web/Media/Formats/codecs_parameter>
    pub struct ContentTypeCodec => &ContentTypeCodecRef;
}

impl InputPathRef {
    /// Examples of path mappings:
    /// ```text
    /// Input Path                                    Output Path
    /// ----------------------------------------------------------------------------------------------------------------
    /// /content/articles/10-months-of-itch/_index.md  /content/articles/10-months-of-itch
    /// /content/posts/hello/world.md                 /content/posts/hello/world
    /// /content/series/rust/part-1/_index.md         /content/series/rust/part-1
    /// /content/about.md                             /content/about
    /// /content/_index.md                            /content
    /// ```
    pub fn folder_path(&self) -> &str {
        self.as_str()
            .trim_end_matches("/_index.md")
            .trim_end_matches(".md")
            .trim_end_matches('/')
    }

    pub fn is_absolute(&self) -> bool {
        self.as_str().starts_with('/')
    }

    /// Returns the canonical path for the given other path, relative to this path.
    /// /// Examples of canonical path mappings:
    /// ```text
    /// base: /content/posts/hello  |  other: foo.png         =>  /content/posts/hello/foo.png
    /// base: /foo/bar             |  other: ../baz.png       =>  /foo/baz.png
    /// base: /articles/intro      |  other: /images/cat.png  =>  /images/cat.png
    /// ```
    pub fn canonicalize_relative_path(&self, other: &Self) -> InputPath {
        if other.is_absolute() {
            other.to_owned()
        } else {
            let folder_path = self.folder_path();
            InputPath::new(format!("{folder_path}/{other}")).resolve()
        }
    }

    /// Resolves `.` and `..` tokens in the given path.
    pub fn resolve(&self) -> InputPath {
        let mut toks = Vec::default();
        for tok in self.as_str().split('/') {
            match tok {
                "." => {
                    // muffin
                }
                ".." => {
                    toks.pop();
                }
                _ => {
                    toks.push(tok);
                }
            }
        }
        InputPath::new(toks.join("/"))
    }
}
impl From<AbsoluteUrl> for Href {
    fn from(url: AbsoluteUrl) -> Self {
        Self::new(url.to_string())
    }
}

macro_rules! impl_safe_minjinja_value_and_from {
    ($type:ty) => {
        #[cfg(feature = "minijinja")]
        impl From<$type> for minijinja::value::Value {
            fn from(value: $type) -> Self {
                minijinja::value::Value::from_safe_string(value.as_str().to_owned())
            }
        }
    };
}

impl_safe_minjinja_value_and_from!(AbsoluteUrl);
impl_safe_minjinja_value_and_from!(InputPath);
impl_safe_minjinja_value_and_from!(Route);
impl_safe_minjinja_value_and_from!(RevisionId);

impl RouteRef {
    /// Build an absolute site URL from this route path (e.g. <https://fasterthanli.me/blah>)
    pub fn to_web_url_string(&self, tc: &TenantConfig, web: WebConfig) -> AbsoluteUrl {
        // e.g. base = `https://fasterthanli.me`
        // e.g. self = `/articles/10-months-of-itch`
        let base = tc.web_base_url(web);
        AbsoluteUrl::new(format!("{base}{self}"))
    }

    /// Build an absolute CDN URL from this route path (e.g. <https://cdn.fasterthanli.me/blah>)
    pub fn to_cdn_url_string(&self, tc: &TenantConfig, web: WebConfig) -> AbsoluteUrl {
        // e.g. base = `https://cdn.fasterthanli.me`
        // e.g. self = `/articles/10-months-of-itch`
        let base = tc.cdn_base_url(web);
        AbsoluteUrl::new(format!("{base}{self}"))
    }

    /// Build an absolute web URL from this route path as a `url::Url`
    pub fn to_web_url(&self, tc: &TenantConfig, web: WebConfig) -> url::Url {
        url::Url::parse(self.to_web_url_string(tc, web).as_str()).unwrap()
    }

    /// Build an absolute CDN URL from this route path as a `url::Url`
    pub fn to_cdn_url(&self, tc: &TenantConfig, web: WebConfig) -> url::Url {
        url::Url::parse(self.to_cdn_url_string(tc, web).as_str()).unwrap()
    }

    /// Trim trailing slash if present
    pub fn trim_trailing_slash(&self) -> Route {
        if self.as_str().ends_with('/') && self.as_str() != "/" {
            let mut r = self.to_owned();
            r.0.pop();
            r
        } else {
            self.to_owned()
        }
    }
}

/// A revision, fully loaded — with tags indexed, series
/// recognized, etc.
#[derive(Clone)]
pub struct Revision {
    /// the original revision
    pub pak: Pak<'static>,

    /// information about the tenant
    pub ti: Arc<TenantInfo>,

    /// loaded pages
    pub pages: HashMap<InputPath, Arc<LoadedPage>>,

    /// maps routes (e.g. `/about`) to page hapas.
    /// aliases just create extra routes.
    pub page_routes: HashMap<Route, InputPath>,

    /// This helps serve `https://cdn.fasterthanli.me/articles/foo/bar~hash.jxl`:
    /// either from memory, or by pointing at an input that may be processed etc.
    pub assets: HashMap<Route, Asset>,

    /// This helps generating cache-busted URLs when rendering HTML. This is mostly
    /// for assets like javascript, css, etc. — media assets (bitmaps, videos, diagrams)
    /// are handled as derivations.
    pub asset_routes: HashMap<InputPath, Route>,

    /// maps tags to page hapas
    pub tags: HashMap<String, Vec<InputPath>>,

    /// media files (including their variants: resized bitmaps, videos, etc.)
    pub media: HashMap<InputPath, Media>,

    /// the path mappings that were used to build that revision, or, failing that, the mappings that
    /// we're going to use to load the revision which will impact... I don't know. I guess we don't
    /// need any path mappings if we receive the revision from mother?
    pub mappings: PathMappings,
}

impl Revision {
    pub fn id(&self) -> &RevisionId {
        &self.pak.id
    }

    pub fn inputs(&self) -> &HashMap<InputPath, Input> {
        &self.pak.inputs
    }
}

impl fmt::Debug for Revision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LoadedRevision {{ id: {:?} }}", self.pak.id)
    }
}

/// A revision pack: contains all inputs, and information about pages, templates,
/// images, etc.
///
/// It still requires processing to be useful: markdown needs to be rendered,
/// SCSS needs to be compiled, etc.
///
/// Some assets are inline (e.g. `Page` has `markup`), and some are in object storage,
/// like images.
#[derive(Clone)]
pub struct Pak<'s> {
    /// revision ID (`rev_{lowercase_ulid}`)
    pub id: RevisionId,

    /// All input files, indexed by path (something like `content/foo/bar.md`)
    pub inputs: HashMap<InputPath, Input>,

    /// Maps paths to the corresponding page (markdown markup)
    /// Markdown rendering and post-processing happens after this.
    /// (but can be cached by Hapa + deps)
    pub pages: HashMap<InputPath, Page<'s>>,

    /// Templates (including shortcodes, partials, etc.)
    pub templates: HashMap<InputPath, Template<'s>>,

    /// Media properties (bitmaps, diagrams, video files, audio files, etc)
    /// like their resolution, etc.
    pub media_props: HashMap<InputPath, MediaProps>,

    /// SVG font face collection
    pub svg_font_face_collection: Arc<SvgFontFaceCollection>,

    /// Revision config (admin github IDs, etc.)
    pub rc: RevisionConfig,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Pak<'s> {
        id, inputs, pages, media_props, templates, svg_font_face_collection, rc
    }
}

#[derive(Clone)]
pub enum Asset {
    // This asset we can serve directly from memory
    Inline {
        content: Bytes,
        content_type: ContentType,
    },

    // This asset is served from object storage, it might go
    // through transcoding, post-processing, etc. but we
    // can get the object storage key from the input path.
    // we already know its mixed hash because we know the
    // hash of all its inputs
    Derivation(Derivation),

    AcceptBasedRedirect {
        options: Vec<(ContentType, Route)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DerivationIdentity;

merde::derive! {
    impl (Serialize, Deserialize) for struct DerivationIdentity { }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DerivationPassthrough;

merde::derive! {
    impl (Serialize, Deserialize) for struct DerivationPassthrough { }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DerivationBitmap {
    pub ic: ICodec,

    // this is an intrinsic width, not CSS pixels (in other words: an `800px@2` image has 1600 pixels).
    // ie. this is the `w` unit in CSS, see https://developer.mozilla.org/en-US/docs/Web/HTML/Responsive_images
    pub width: Option<IntrinsicPixels>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DerivationBitmap { ic, width }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DerivationVideo {
    pub container: VContainer,
    pub vc: VCodec,
    pub ac: ACodec,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DerivationVideo { container, vc, ac }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DerivationVideoThumbnail {
    pub ic: ICodec,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DerivationVideoThumbnail { ic }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DerivationDrawioRender {
    pub svg_font_face_collection: Arc<SvgFontFaceCollection>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DerivationDrawioRender {
        svg_font_face_collection
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SvgFontFaceCollection {
    pub faces: Vec<SvgFontFace>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct SvgFontFaceCollection { faces }
}

/// SVG font-face definition
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SvgFontFace {
    pub family: String,

    // the font's weight
    pub weight: FontWeight,

    // the font's style
    pub style: FontStyle,

    // the file name, something like `Iosevka.woff2`
    pub file_name: String,

    // hash of the font file
    pub hash: InputHash,

    // the font itself, encoded as bytes (woff2)
    pub contents: Vec<u8>,
}

impl SvgFontFace {
    /// Returns the full name of the font face, including its
    /// family and weight, font style if any etc.
    pub fn full_name(&self) -> String {
        format!("{}-{:?}", self.family, self.weight)
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for struct SvgFontFace {
        family, weight, style, file_name, hash, contents
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DerivationSvgCleanup {
    // no font faces needed
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DerivationSvgCleanup {}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DerivationKind {
    /// not doing anything special _and_ not cache-busted!
    Passthrough(DerivationPassthrough),

    /// not doing anything special
    Identity(DerivationIdentity),

    /// transcoding JXL/PNG to AVIF, WEBP
    Bitmap(DerivationBitmap),

    /// transcoding AV1 to VP9 etc.
    Video(DerivationVideo),

    /// grabbing the first frame of video as a thumbnail
    VideoThumbnail(DerivationVideoThumbnail),

    /// transcoding SVG to PNG
    DrawioRender(DerivationDrawioRender),

    /// injecting viewbox in SVG, minifying, etc.
    SvgCleanup(DerivationSvgCleanup),
}

merde::derive! {
    impl (Serialize, Deserialize) for enum DerivationKind externally_tagged {
        "Passthrough" => Passthrough,
        "Identity" => Identity,
        "Bitmap" => Bitmap,
        "Video" => Video,
        "VideoThumbnail" => VideoThumbnail,
        "DrawioRender" => DrawioRender,
        "SvgCleanup" => SvgCleanup,
    }
}

impl std::fmt::Display for DerivationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DerivationKind::Passthrough(_) => write!(f, "passthrough"),
            DerivationKind::Identity(_) => write!(f, "ident"),
            DerivationKind::Bitmap(bitmap) => write!(f, "bitmap({})", bitmap.ic),
            DerivationKind::Video(video) => {
                write!(f, "video({}+{} in {})", video.vc, video.ac, video.container)
            }
            DerivationKind::VideoThumbnail(thumbnail) => {
                write!(f, "videothumb({})", thumbnail.ic)
            }
            DerivationKind::DrawioRender(_) => write!(f, "drawio"),
            DerivationKind::SvgCleanup(_) => write!(f, "svgcleanup"),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Part {
    pub title: String,
    pub path: InputPath,
    pub route: Route,
}

/// A 1-based series part number
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct PartNumber(usize);

impl fmt::Display for PartNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0 + 1)
    }
}

impl PartNumber {
    pub fn new(n: usize) -> Self {
        assert!(n >= 1);
        Self(n)
    }

    pub fn as_usize(&self) -> usize {
        self.0 - 1
    }
}

/// An input file for a revision, including markdown files,
/// SASS files, images, .drawio files, etc.
#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub struct Input {
    // input hash
    pub hash: InputHash,

    // input path, like `content/articles/10-months-of-itch/_index.md`
    pub path: InputPath,

    // last modified time
    pub mtime: Rfc3339<OffsetDateTime>,

    // size in bytes
    pub size: u64,

    // content-type
    pub content_type: ContentType,
}

impl Input {
    pub fn base(&self) -> &str {
        let (base, _ext) = self.path.explode();
        base
    }

    pub fn ext(&self) -> &str {
        let (_base, ext) = self.path.explode();
        ext
    }

    pub fn key(&self) -> ObjectStoreKey {
        input_key(self.hash.as_str(), self.ext())
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Input {
        hash, path, mtime, size, content_type
    }
}

#[derive(Clone)]
pub struct Page<'s> {
    pub hash: InputHash,
    pub path: InputPath,

    // the markdown content (including frontmatter)
    pub markup: CowStr<'s>,

    // dependencies (images, shortcodes, etc.)
    pub deps: Vec<InputPath>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Page<'s> {
        hash, path, markup, deps
    }
}

#[derive(Clone)]
pub struct Template<'s> {
    pub path: InputPath,

    /// jinja markup
    pub markup: CowStr<'s>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Template<'s> {
        path, markup
    }
}

#[derive(Clone)]
pub struct Stylesheet<'s> {
    pub path: InputPath,

    /// SCSS markup
    pub markup: CowStr<'s>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Stylesheet<'s> {
        path, markup
    }
}

#[derive(Default)]
pub struct SearchResults {
    pub results: Vec<SearchResult>,
    pub terms: Vec<String>,
    pub num_results: usize,
    pub has_more: bool,
}

impl std::fmt::Debug for SearchResults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchResults")
            .field("terms", &self.terms)
            .field("num_results", &self.num_results)
            .finish()
    }
}

#[derive(Clone)]
pub struct SearchResult {
    /// The path itself
    pub page: Arc<LoadedPage>,

    /// The title, with terms highlighted as `<em>` tags
    pub title_snippet: Html,

    /// Parts of the body that matched the query terms, highlighted, as HTML
    pub body_snippet: Html,

    /// Text fragments, cf. <https://developer.mozilla.org/en-US/docs/Web/URI/Reference/Fragment/Text_fragments>
    /// Already joined by `&` and prefixed with `:~:`, all that needs to be done is to put it after a `#`
    pub fragments: String,
}

impl std::fmt::Debug for SearchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchResult")
            .field("page", &self.page)
            .field("title_snippet", &self.title_snippet)
            .field("body_snippet", &self.body_snippet)
            .field("fragments", &self.fragments)
            .finish()
    }
}

pub struct Completion {
    pub kind: CompletionKind,
    pub text: String,
    pub html: Html,
    pub url: Option<Href>,
}

merde::derive! {
    impl (Serialize, ) for struct Completion {
        kind, text, html, url
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    Term,
    Article,
}

merde::derive! {
    impl (Serialize, ) for enum CompletionKind string_like {
        "term" => Term,
        "article" => Article,
    }
}

/// Maps input paths to disk paths
#[derive(Clone, Debug)]
pub struct PathMapping {
    input_path: InputPath,
    disk_path: Utf8PathBuf,
}

/// Complete set of mapping between input paths (e.g. `/content/blah.md`)
/// and disk paths (e.g. `/var/www/content/blah.md`)
#[derive(Default, Clone, Debug)]
pub struct PathMappings {
    entries: Vec<PathMapping>,
}

pub const ROOT_INPUT_PATHS: [&InputPathRef; 3] = [
    InputPathRef::from_static("/home.json"),
    InputPathRef::from_static("/content"),
    InputPathRef::from_static("/templates"),
];

impl PathMappings {
    /// Creates a new PathMappings with a default mapping from `/content` to the content directory.
    pub fn from_ti(ti: &TenantInfo) -> Self {
        Self {
            entries: ROOT_INPUT_PATHS
                .iter()
                .map(|&path| PathMapping {
                    input_path: InputPath::from(path),
                    disk_path: ti.base_dir.join(path.as_str().trim_start_matches('/')),
                })
                .collect(),
        }
    }

    pub fn add(&mut self, input_path: InputPath, disk_path: Utf8PathBuf) {
        self.entries.push(PathMapping {
            input_path,
            disk_path,
        });
    }

    /// Converts an input path to a disk path by finding a matching prefix mapping and applying it.
    ///
    /// For example, if there is a mapping from `/content` to `/var/lib/home/tenant/content`,
    /// then calling `to_disk_path("/content/_index.md")` would return a path like
    /// `/var/lib/home/tenant/content/_index.md`.
    pub fn to_disk_path_maybe(&self, input_path: &InputPathRef) -> Option<Utf8PathBuf> {
        for entry in &self.entries {
            if input_path.as_str().starts_with(entry.input_path.as_str()) {
                let relative_path = &input_path.as_str()[entry.input_path.as_str().len()..];
                let relative_path = relative_path.trim_start_matches('/');
                if relative_path.is_empty() {
                    return Some(entry.disk_path.clone());
                }
                return Some(entry.disk_path.join(relative_path));
            }
        }
        None
    }

    /// Converts a disk path to an input path by finding a matching prefix mapping and applying it.
    ///
    /// For example, if there is a mapping from `/content` to `/var/lib/home/tenant/content`,
    /// then calling `to_input_path("/var/lib/home/tenant/content/_index.md")` would return a path like
    /// `/content/_index.md`.
    pub fn to_input_path_maybe(&self, disk_path: &Utf8Path) -> Option<InputPath> {
        for entry in &self.entries {
            if disk_path.as_str().starts_with(entry.disk_path.as_str()) {
                let relative_path = &disk_path.as_str()[entry.disk_path.as_str().len()..];
                let relative_path = relative_path.trim_start_matches('/');

                if relative_path.is_empty() {
                    return Some(entry.input_path.clone());
                }

                return Some(InputPath::new(format!(
                    "{}/{relative_path}",
                    entry.input_path
                )));
            }
        }
        None
    }

    pub fn to_disk_path(&self, input_path: &InputPathRef) -> eyre::Result<Utf8PathBuf> {
        self.to_disk_path_maybe(input_path).ok_or_else(|| {
            eyre::eyre!(
                "Failed to map input path {} to disk path. Available mappings: {}",
                input_path,
                self.entries
                    .iter()
                    .map(|e| format!("input: {} -> disk: {}", e.input_path, e.disk_path))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }

    pub fn to_input_path(&self, disk_path: &Utf8Path) -> eyre::Result<InputPath> {
        self.to_input_path_maybe(disk_path).ok_or_else(|| {
            eyre::eyre!(
                "Failed to map disk path {} to input path. Available mappings: {}",
                disk_path,
                self.entries
                    .iter()
                    .map(|e| format!("input: {} -> disk: {}", e.input_path, e.disk_path))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(feature = "serde")]
    fn test_input_path_roundtrip() {
        use camino::Utf8PathBuf;
        use config::{TenantConfig, TenantInfo};

        use crate::{InputPath, PathMappings};

        // Create a dummy tenant config and then set the base_dir, which is the only property we care about
        let ti = TenantInfo {
            base_dir: "/ftl".into(),
            tc: TenantConfig {
                name: "fastgerthanli.me".into(),
                domain_aliases: vec![],
                object_storage: None,
                secrets: None,
            },
        };

        let mut mappings = PathMappings::from_ti(&ti);
        let content_dir = ti.content_dir();

        // Test content path
        let content_path = content_dir.join("articles/test-article.md");
        let input_path = mappings.to_input_path_maybe(&content_path).unwrap();
        assert_eq!(input_path.as_str(), "/content/articles/test-article.md");
        let disk_path = mappings.to_disk_path(&input_path).unwrap();
        assert_eq!(disk_path, content_path);

        // Add a mapping for frontend dist path
        let dist_path = Utf8PathBuf::from("/tmp/dist-tmp32");
        mappings.add(InputPath::new("/dist".to_string()), dist_path.clone());

        // Test frontend dist path
        let frontend_path = dist_path.join("js/main.js");
        let input_path = mappings.to_input_path_maybe(&frontend_path).unwrap();
        assert_eq!(input_path.as_str(), "/dist/js/main.js");
        let disk_path = mappings.to_disk_path(&input_path).unwrap();
        assert_eq!(disk_path, frontend_path);

        // Test invalid path
        let invalid_path = Utf8PathBuf::from("/some/random/path");
        assert!(mappings.to_input_path_maybe(&invalid_path).is_none());
    }

    #[test]
    fn test_pathmappings() {
        use super::*;
        use camino::Utf8PathBuf;

        let mut mappings = PathMappings::default();
        mappings.add(
            InputPath::new("/content".to_string()),
            Utf8PathBuf::from("/var/lib/home/tenant/content"),
        );
        mappings.add(
            InputPath::new("/dist".to_string()),
            Utf8PathBuf::from("/tmp/dist-tmp20991"),
        );

        // Test to_disk_path
        let input_path = InputPathRef::from_str("/content/articles/test-article.md");
        let disk_path = mappings.to_disk_path(input_path).unwrap();
        assert_eq!(
            disk_path,
            Utf8PathBuf::from("/var/lib/home/tenant/content/articles/test-article.md")
        );

        let input_path = InputPathRef::from_str("/dist/js/main.js");
        let disk_path = mappings.to_disk_path(input_path).unwrap();
        assert_eq!(
            disk_path,
            Utf8PathBuf::from("/tmp/dist-tmp20991/js/main.js")
        );

        // Test nonexistent mapping
        let input_path = InputPathRef::from_str("/nonexistent/path.txt");
        assert!(mappings.to_disk_path_maybe(input_path).is_none());

        // Test to_input_path
        let disk_path = Utf8PathBuf::from("/var/lib/home/tenant/content/articles/test-article.md");
        let input_path = mappings.to_input_path_maybe(&disk_path).unwrap();
        assert_eq!(input_path.as_str(), "/content/articles/test-article.md");

        let disk_path = Utf8PathBuf::from("/tmp/dist-tmp20991/js/main.js");
        let input_path = mappings.to_input_path_maybe(&disk_path).unwrap();
        assert_eq!(input_path.as_str(), "/dist/js/main.js");

        // Test nonexistent mapping
        let disk_path = Utf8PathBuf::from("/some/random/path.txt");
        assert!(mappings.to_input_path_maybe(&disk_path).is_none());

        // Test exact path matches
        let input_path = InputPathRef::from_str("/content");
        let disk_path = mappings.to_disk_path(input_path).unwrap();
        assert_eq!(disk_path, Utf8PathBuf::from("/var/lib/home/tenant/content"));

        let disk_path = Utf8PathBuf::from("/var/lib/home/tenant/content");
        let input_path = mappings.to_input_path_maybe(&disk_path).unwrap();
        assert_eq!(input_path.as_str(), "/content");
    }
}
