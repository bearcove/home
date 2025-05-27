use conflux::{PodcastMeta, Route};
use facet::Facet;
use time::OffsetDateTime;


#[derive(Facet, Debug)]
pub struct Frontmatter {
    /// Title of the page
    pub title: String,

    /// An optional subtitle, usable for punchy asides of the main title
    #[facet(default)]
    pub subtitle: Option<String>,

    /// Jinja2 template to use for rendering — defaults to `page.html`
    #[facet(default = "page.html".into())]
    pub template: String,

    /// Publication date in RFC3339 format, e.g. `2023-10-01T12:00:00Z` (UTC)
    pub date: OffsetDateTime,

    /// Last update date, if any
    #[facet(default)]
    pub updated_at: Option<OffsetDateTime>,

    /// If true, page is only visible by admins
    #[facet(default = false)]
    pub draft: bool,

    /// Whether the page should be excluded from search indexing
    #[facet(default = false)]
    pub archive: bool,

    /// Code used to allow access to a draft
    #[facet(rename = "draft-code", default)]
    pub draft_code: Option<String>,

    /// Alternative routes for this page (for redirects)
    #[facet(default)]
    pub aliases: Vec<Route>,

    /// Tags associated with the page (useful for listings)
    #[facet(default)]
    pub tags: Vec<String>,

    /// Items associated with podcast episodes
    #[facet(default)]
    pub podcast: Option<PodcastMeta>,

    /// Additional metadata for the page
    #[facet(default)]
    pub extra: FrontmatterExtras,
}

#[derive(Facet, Default, Debug)]
#[facet(default)]
pub struct FrontmatterExtras {
    // show patreon credits
    pub patreon: bool,

    // don't show reddit comments button
    pub hide_comments: bool,

    // don't show patreon plug
    pub hide_patreon: bool,

    // don't show date, author, etc.
    pub hide_metadata: bool,

    // tube slug
    pub tube: Option<String>,

    // youtube video ID
    pub youtube: Option<String>,

    // whether this is a dual feature (show the video while the article is still exclusive)
    pub dual_feature: bool,

    // video duration
    pub duration: Option<u64>,

    // for a series, marks whether it's still ongoing
    pub ongoing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FrontmatterExtrasIn {
    pub patreon: Option<bool>,
    pub hide_comments: Option<bool>,
    pub hide_patreon: Option<bool>,
    pub hide_metadata: Option<bool>,
    pub tube: Option<String>,
    pub dual_feature: Option<bool>,
    pub youtube: Option<String>,
    pub duration: Option<u64>,
    pub ongoing: Option<bool>,
}

impl From<FrontmatterExtrasIn> for FrontmatterExtras {
    fn from(frontmatter_extras_in: FrontmatterExtrasIn) -> Self {
        Self {
            patreon: frontmatter_extras_in.patreon.unwrap_or_default(),
            hide_comments: frontmatter_extras_in.hide_comments.unwrap_or_default(),
            hide_patreon: frontmatter_extras_in.hide_patreon.unwrap_or_default(),
            hide_metadata: frontmatter_extras_in.hide_metadata.unwrap_or_default(),
            tube: frontmatter_extras_in.tube,
            dual_feature: frontmatter_extras_in.dual_feature.unwrap_or_default(),
            youtube: frontmatter_extras_in.youtube,
            duration: frontmatter_extras_in.duration,
            ongoing: frontmatter_extras_in.ongoing.unwrap_or_default(),
        }
    }
}
